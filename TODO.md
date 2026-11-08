# TODO

## Next

- [ ] Browse: loader and game version filters outside the "install into instance" mode

## Later

- [ ] Use the pack's icon for imported `.mrpack` files (only Browse installs get the Modrinth icon now)
- [ ] Microsoft login: device code flow as a fallback when the browser redirect can't work
- [ ] Settings: Java installations (pick a Java per instance, use a system JRE, Linux arm64 has no Mojang runtime)
- [ ] Settings: memory per instance and a default (instance.json already has `maxMemoryMb`)
- [ ] Forge before 1.13 (old installer format with `versionInfo`)
- [ ] "Support Forge" link on Forge instances (Forge's version.json asks automated installers for it)
- [ ] Cancel an install that's still preparing

## Before release

- [ ] Own Microsoft Entra app + Minecraft API approval (https://aka.ms/mce-reviewappid); the Prism client ID is dev-only and release builds refuse it
- [ ] Bump `rust-version` if newer deps need it (keyring 4, zip 7.4 want 1.88+)
