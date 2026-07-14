# portable-pty local patch

Ricochet vendors portable-pty 0.9.0 from crates.io under its MIT license.

- Upstream repository: https://github.com/wezterm/wezterm
- Crates.io package: portable-pty 0.9.0
- Crates.io archive SHA-256: b4a596a2b3d2752d94f51fac2d4a96737b8705dddd311a32b9af47211f08671e
- Imported: 2026-07-14

## Ricochet delta

The Windows CreatePseudoConsole call does not set PSEUDOCONSOLE_INHERIT_CURSOR.
Ricochet cannot service the cursor-position query during the blocking openpty call
because its PTY reader starts only after openpty returns. The resize-quirk and
Win32-input-mode flags remain unchanged.

All other source is portable-pty 0.9.0.
