// Stand-in for the Weapon_Package_Explosive2 port tests (CC0): our own
// launchers on the stand-in Tier 1, under the method names the port covers.
if(ForceRequiredAddOn("Weapon_Package_Tier1") == $Error::AddOn_NotFound)
{
   error("Stand-in Explosive 2 needs the stand-in Tier 1");
}
else if(!isFile("add-ons/weapon_rocket_launcher/server.cs"))
{
   error("Stand-in Explosive 2 needs the base game's rocket launcher");
}
else
{
   exec("./Weapon_RPG.cs");
   exec("./Weapon_Grenade_Launcher.cs");
   exec("./Support_PrjLoop.cs");
   exec("./Weapon_Calibre_Cannon.cs");
   if($Pref::Server::TT::EasterEgg)
   {
      exec("./Weapon_Mortar.cs");
   }
}
