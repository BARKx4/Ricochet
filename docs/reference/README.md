# Ricochet Reference Website

Open index.html in a browser to read the static reference site.

From the repository root:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File docs\reference\validate.ps1
```

To run the broader local acceptance suite after building `rco`:

```powershell
cargo build -p ricochet_cli --bin rco
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts\acceptance.ps1
```

The acceptance script validates this static site, runs the shipped examples, creates a fresh MVC scaffold, then runs `rco check` and `rco test` against it. It leaves the generated scaffold in a temp folder for inspection.

The site is intentionally static: no build step, no package install, and no server required.
