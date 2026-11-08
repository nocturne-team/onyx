# TODO
Don't forget to tick the boxes in this file!

## Next

- [x] Filter content browser by categories (i.e. Decoration, etc)

## Later

- [x] Cache mod icons? (currently it takes a few seconds for them to appear on the mod list screen)
- [ ] Bring the UI closer to Modrinth's App
- [ ] Pass over the dependencies and check if we can remove any
- [ ] Optimize pass + make the app more lightweight
- [ ] Forge before 1.13 (old installer format with `versionInfo`)
- [ ] Selective prism launcher instance import
- [x] Discord Rich Presence
- [ ] Total physical memory on Windows (the memory slider falls back to a 16 GB cap there)
- [ ] Discord invites...... (either by using e4mc or playitgg[would probably require the user to login into the app]) this also means registering a protocol (i.e. onyx://join-game/{mc-version}/ and that'd either pop up an instance selector or choose the closest one)

## Before release

- [ ] Signed releases 
- [ ] Github actions best practises (i.e. dependabot / codeql etc) + release
- [ ] Own Microsoft Entra app + Minecraft API approval (https://aka.ms/mce-reviewappid); the Prism client ID is dev-only and release builds refuse it
- [ ] Bump `rust-version` if newer deps need it (keyring 4, zip 7.4 want 1.88+)
- [ ] Exe icon on windows, .desktop entry on linux
- [ ] Arch Linux Pacman package

## Skipped / Postponed heavily

- [~] Microsoft login: device code flow as a fallback when the browser redirect can't work
- [ ] Flatpack package
