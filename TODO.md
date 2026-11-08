# TODO

## Next

- [ ] Microsoft login: device code flow as a fallback when the browser redirect can't work

## Later

- [ ] Forge before 1.13 (old installer format with `versionInfo`)
- [ ] Total physical memory on Windows (the memory slider falls back to a 16 GB cap there)

## Before release

- [ ] Own Microsoft Entra app + Minecraft API approval (https://aka.ms/mce-reviewappid); the Prism client ID is dev-only and release builds refuse it
- [ ] Bump `rust-version` if newer deps need it (keyring 4, zip 7.4 want 1.88+)
