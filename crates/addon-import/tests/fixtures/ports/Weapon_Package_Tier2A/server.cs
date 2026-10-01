// Stand-in for the Weapon_Package_Tier2A port tests (CC0): our own guns on
// the stand-in Tier 1, under the datablock and method names the port covers.
%error = ForceRequiredAddOn("Weapon_Package_Tier1");

if(%error == $Error::AddOn_NotFound)
{
   error("ERROR: Weapon_Package_Tier2A - required add-on Weapon_Package_Tier1 not found");
}
else
{
   exec("./Weapon_Guns.cs");
   if($Pref::Server::TT::EasterEgg)
   {
      exec("./Weapon_Match.cs");
   }
   // exec("./Weapon_Unused.cs");
}
