# Assets and host rules in one Add-On

An author can keep content and native host rules in one folder, check that
folder, and distribute one ZIP. This uses the existing package loader and
companion lifecycle; no TorqueScript is executed. See the working original
sample in `packages/samples/sample-co-located`.

```text
MyAddon/
  addon.json
  content/
    package.json
    marker.json
  server/
    package.json
    behaviour.json
    main.rhai
```

`addon.json` is a versioned layout descriptor:

```json
{ "schema_version": 1, "content": "content", "server": "server" }
```

Both paths are distinct direct child folders with their own `package.json`.
`server` is optional. Each component retains its authored package ID, version,
namespace and capabilities. Content is client or shared data; rules are
server-only. The content manifest lists the rules ID in `companions`, and the
rules manifest declares a dependency on the content ID. These explicit
relationships drive the existing single Add-On row and enable/disable behavior.
Folder names and a `-rules` suffix do not grant execution rights.

```sh
bri-addon-check MyAddon
bri-addon-check MyAddon --package --out MyAddon.zip
# Optional CLI staging without legacy conversion:
bri-import-addon native MyAddon FreshOutput
```

Drop the folder or ZIP into the game's `Add-Ons` folder. The installer stages
the two components into its normal content/rules locations and installs them
together. The managed installation may use two internal directories; authors
and players manage one source folder or ZIP and one Add-On choice. Replacing or
removing that source uses the existing transactional companion lifecycle.

Only the content component is offered to joining clients. Host scripts never
enter its file listing or content hash. Changing host rules does not change
the downloadable content's identity. Sandboxed client code still requires its
ordinary declared capabilities and trust; co-location grants no extra access.

Existing standalone packages and separate companion folders/ZIPs remain
supported. This layout does not combine independently managed dependencies
or change their identities. Keep files inside the declared component folders;
the container's `addon.json` is authoring metadata, not runtime content.
