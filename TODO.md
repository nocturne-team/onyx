# TODO

## Next

- [ ] Throttle downloads, I just downloaded Faboulously Optimised in less than a scond

## Later

- [ ] Filter content browser by categories (i.e. Decoration, etc)
- [ ] Cache mod icons? (currently it takes a few seconds for them to appear on the mod list screen)
- [ ] Bring the UI closer to Modrinth's App
- [ ] Forge before 1.13 (old installer format with `versionInfo`)
- [ ] Total physical memory on Windows (the memory slider falls back to a 16 GB cap there)
- [ ] Selective prism launcher instance import

## Before release

- [ ] Signed releases 
- [ ] Github actions best practises (i.e. dependabot / codeql etc) + release
- [ ] Own Microsoft Entra app + Minecraft API approval (https://aka.ms/mce-reviewappid); the Prism client ID is dev-only and release builds refuse it
- [ ] Bump `rust-version` if newer deps need it (keyring 4, zip 7.4 want 1.88+)
- [ ] Exe icon on windows, .desktop entry on linux
- [ ] ArchLinux Pacman package

## Skipped / Postponed heavily

- [~] Microsoft login: device code flow as a fallback when the browser redirect can't work
- [ ] Flatpack package
