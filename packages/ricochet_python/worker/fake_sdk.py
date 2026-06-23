"""Small fake Python SDK used by @ricochet/python package tests."""


VERSION = "1.0-test"


def add(left, right):
    return left + right


def echo(value):
    return {"echo": value}


def noisy(value):
    print(f"stdout noise: {value}")
    return f"clean {value}"


def raise_error():
    raise RuntimeError("boom from fake SDK")


class Client:
    def __init__(self, name, token=None):
        self.name = name
        self.token = token

    def greeting(self, target):
        return f"{self.name} says hello to {target}"

    def add(self, left, right):
        return left + right

    def settings(self):
        return {"name": self.name, "token_set": self.token is not None}
