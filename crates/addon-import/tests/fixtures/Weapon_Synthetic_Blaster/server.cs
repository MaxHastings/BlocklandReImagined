// Synthetic Add-On for bri-addon-import tests. Shaped like community weapons:
// a hard dependency, a custom multi-shot onFire, a brick, a sound and a
// package that overrides a global callback.
%error = ForceRequiredAddOn("Weapon_Gun");

if(%error == $Error::AddOn_Disabled)
   GunItem.uiName = "";

if(%error == $Error::AddOn_NotFound)
   error("ERROR: Weapon_Synthetic_Blaster - required add-on Weapon_Gun not found");
else
   exec("./blaster.cs");

exec("./bricks/pad.cs");
exec("./missing.cs");
