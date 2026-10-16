# AvP public release exclusion

Maxwell requested that AvP remain a local test map and not ship in v0.3.2.
The existing default-Add-On `original.withdrawn` field now excludes `map_avp`
(Map_AvP; provider `map_avp:map/map_avp`) without changing runtime/import code
or deleting source/generated local copies. Windows, macOS and Linux packagers
all use `addon_bundle.default_sources`, which filters this field.

Read-only validation against the existing primary release bundle selected 160
package/companion entries, none named map_avp. Selected manifests contain no
map_avp dependency or provider. The existing primary local package still exists.
No local bundle install/cleanup was run: that operation removes withdrawn
bundled imports and would defeat preserving the current local test copy.

Release preparation must refresh bundle/credits and independently inspect final
archives for the excluded package/provider/resources. This scoped selection check
is not an extracted-archive or full integrated gate pass. v0.3.1 is unchanged.
