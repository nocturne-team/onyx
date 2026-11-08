# TODO

## Next

- [ ] Settings: Java installations (pick a Java per instance, use a system JRE, Linux arm64 has no Mojang runtime)

## Later

- [ ] Use the pack's icon for imported `.mrpack` files (only Browse installs get the Modrinth icon now)
- [ ] Microsoft login: device code flow as a fallback when the browser redirect can't work
- [ ] Forge before 1.13 (old installer format with `versionInfo`)
- [ ] "Support Forge" link on Forge instances (Forge's version.json asks automated installers for it)
- [ ] Cancel an install that's still preparing

- [ ] Total physical memory on Windows (the memory slider falls back to a 16 GB cap there)

## Before release

- [ ] Own Microsoft Entra app + Minecraft API approval (https://aka.ms/mce-reviewappid); the Prism client ID is dev-only and release builds refuse it
- [ ] Bump `rust-version` if newer deps need it (keyring 4, zip 7.4 want 1.88+)
