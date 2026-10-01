// Stand-in for the Weapon_ShortRifleKai port tests (CC0): our own bouncing
// rifle on the stand-in Tier 1, under the datablock and method names the
// port covers. Its crit damage type has the name the Adventurer's Weapons
// stand-in gives its own, with another kill message: each keeps its own.
if(ForceRequiredAddOn("Weapon_Package_Tier1") == $Error::AddOn_NotFound)
   error("Weapon_ShortRifleKai stand-in: no Weapon_Package_Tier1");
else
{
   exec("./tracer.cs");
   exec("./raycast.cs");
   exec("./rifle.cs");
}
