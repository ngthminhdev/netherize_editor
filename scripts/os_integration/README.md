# Netherize OS Integration

This folder keeps OS-facing packaging helpers separate from the editor runtime.

## macOS app bundle

Build a local `.app` bundle:

```bash
scripts/os_integration/bundle_macos.sh
```

The generated `target/Netherize.app` registers Netherize as an editor for:

- `public.plain-text`
- `public.text`
- `public.source-code`

After copying the app to `/Applications`, refresh LaunchServices if Finder does
not show Netherize immediately:

```bash
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister \
  -f /Applications/Netherize.app
```

Manual checks:

```bash
open -a Netherize /tmp/a.txt
open /Applications/Netherize.app
```

## CLI alias

The Rust package and default binary remain `netherize_editor`. The root
`scripts/install.sh` creates a `netherize` symlink in `~/.local/bin`, so this
works after install:

```bash
netherize /tmp/a.txt
```
