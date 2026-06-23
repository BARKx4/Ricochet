#!/usr/bin/env python3
"""JSON-lines worker used by @ricochet/python."""

from __future__ import annotations

import importlib
import inspect
import json
import sys
import traceback
from contextlib import redirect_stdout
from types import ModuleType
from typing import Any


REF_KEY = "__ricochet_py_ref"


class Bridge:
    def __init__(self) -> None:
        self._refs: dict[str, Any] = {}
        self._next_ref = 1

    def serve(self) -> None:
        protocol_stdout = sys.stdout
        for raw_line in sys.stdin:
            line = raw_line.strip()
            if not line:
                continue
            should_stop = False
            try:
                request = json.loads(line)
                with redirect_stdout(sys.stderr):
                    response, should_stop = self._handle_request(request)
            except Exception as exc:  # noqa: BLE001 - the bridge must report all worker faults.
                response = self._error_response(None, exc)
            protocol_stdout.write(json.dumps(response, separators=(",", ":")) + "\n")
            protocol_stdout.flush()
            if should_stop:
                break

    def _handle_request(self, request: dict[str, Any]) -> tuple[dict[str, Any], bool]:
        request_id = request.get("id")
        op = request.get("op")
        payload = request.get("payload") or {}
        try:
            if op == "import":
                value = importlib.import_module(str(payload["module"]))
            elif op == "getattr":
                value = getattr(self._resolve_target(payload["target"]), str(payload["name"]))
            elif op == "setattr":
                setattr(
                    self._resolve_target(payload["target"]),
                    str(payload["name"]),
                    self._decode(payload.get("value")),
                )
                value = True
            elif op == "call":
                target = self._resolve_target(payload["target"])
                value = target(
                    *self._decode(payload.get("args", [])),
                    **self._decode(payload.get("kwargs", {})),
                )
            elif op == "construct":
                cls = self._resolve_dotted(str(payload["class"]))
                value = cls(
                    *self._decode(payload.get("args", [])),
                    **self._decode(payload.get("kwargs", {})),
                )
            elif op == "method":
                method = getattr(self._resolve_target(payload["target"]), str(payload["name"]))
                value = method(
                    *self._decode(payload.get("args", [])),
                    **self._decode(payload.get("kwargs", {})),
                )
            elif op == "dir":
                value = sorted(dir(self._resolve_target(payload["target"])))
            elif op == "inspect":
                value = self._inspect_value(self._resolve_target(payload["target"]))
            elif op == "exports":
                value = self._exports(
                    str(payload["module"]),
                    bool(payload.get("include_private", False)),
                )
            elif op == "release":
                value = self._release(payload.get("refs", []))
            elif op == "shutdown":
                return self._ok_response(request_id, True), True
            else:
                raise ValueError(f"unknown Python bridge operation: {op!r}")
            return self._ok_response(request_id, self._encode(value)), False
        except Exception as exc:  # noqa: BLE001 - exceptions belong in the protocol envelope.
            return self._error_response(request_id, exc), False

    def _resolve_target(self, target: Any) -> Any:
        decoded = self._decode(target)
        if isinstance(decoded, str):
            return self._resolve_dotted(decoded)
        return decoded

    def _resolve_dotted(self, dotted: str) -> Any:
        if not dotted:
            raise ValueError("empty Python target")
        parts = dotted.split(".")
        for index in range(len(parts), 0, -1):
            module_name = ".".join(parts[:index])
            try:
                value: Any = importlib.import_module(module_name)
            except ModuleNotFoundError:
                continue
            for attr in parts[index:]:
                value = getattr(value, attr)
            return value
        raise ModuleNotFoundError(dotted)

    def _decode(self, value: Any) -> Any:
        if isinstance(value, dict):
            ref = value.get(REF_KEY)
            if ref is not None:
                try:
                    return self._refs[str(ref)]
                except KeyError as exc:
                    raise KeyError(f"unknown Python reference: {ref}") from exc
            return {key: self._decode(item) for key, item in value.items()}
        if isinstance(value, list):
            return [self._decode(item) for item in value]
        return value

    def _encode(self, value: Any) -> Any:
        if value is None or isinstance(value, (bool, int, float, str)):
            return value
        if isinstance(value, (list, tuple)):
            return [self._encode(item) for item in value]
        if isinstance(value, set):
            return [self._encode(item) for item in sorted(value, key=repr)]
        if isinstance(value, dict) and all(isinstance(key, str) for key in value):
            return {key: self._encode(item) for key, item in value.items()}
        return self._reference(value)

    def _reference(self, value: Any) -> dict[str, str]:
        ref_id = f"py{self._next_ref}"
        self._next_ref += 1
        self._refs[ref_id] = value
        return {
            REF_KEY: ref_id,
            "type": self._type_name(value),
            "repr": repr(value),
        }

    def _type_name(self, value: Any) -> str:
        if isinstance(value, ModuleType):
            return "module"
        if inspect.isclass(value):
            return "class"
        if inspect.isfunction(value) or inspect.ismethod(value) or callable(value):
            return "callable"
        return type(value).__name__

    def _inspect_value(self, value: Any) -> dict[str, Any]:
        signature = None
        if callable(value):
            try:
                signature = str(inspect.signature(value))
            except (TypeError, ValueError):
                signature = None
        doc = inspect.getdoc(value)
        return {
            "type": self._type_name(value),
            "callable": callable(value),
            "signature": signature,
            "doc": doc.splitlines()[0] if doc else None,
            "repr": repr(value),
        }

    def _exports(self, module_name: str, include_private: bool) -> list[dict[str, Any]]:
        module = importlib.import_module(module_name)
        exports: list[dict[str, Any]] = []
        for name in sorted(dir(module)):
            if not include_private and name.startswith("_"):
                continue
            try:
                value = getattr(module, name)
            except Exception:
                continue
            item = self._inspect_value(value)
            item["name"] = name
            item["dotted"] = f"{module_name}.{name}"
            exports.append(item)
        return exports

    def _release(self, refs: Any) -> dict[str, Any]:
        released: list[str] = []
        for item in self._decode_ref_list(refs):
            if item in self._refs:
                del self._refs[item]
                released.append(item)
        return {"released": released, "remaining": len(self._refs)}

    def _decode_ref_list(self, refs: Any) -> list[str]:
        if not isinstance(refs, list):
            return []
        decoded: list[str] = []
        for item in refs:
            if isinstance(item, dict) and REF_KEY in item:
                decoded.append(str(item[REF_KEY]))
            else:
                decoded.append(str(item))
        return decoded

    def _ok_response(self, request_id: Any, value: Any) -> dict[str, Any]:
        return {"id": request_id, "ok": True, "value": value}

    def _error_response(self, request_id: Any, exc: BaseException) -> dict[str, Any]:
        return {
            "id": request_id,
            "ok": False,
            "error": {
                "kind": "PythonException",
                "message": f"{type(exc).__name__}: {exc}",
                "traceback": traceback.format_exc(),
            },
        }


if __name__ == "__main__":
    Bridge().serve()
