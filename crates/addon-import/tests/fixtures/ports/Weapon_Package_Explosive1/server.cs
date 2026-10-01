// Stand-in for the Weapon_Package_Explosive1 port tests (CC0): our own
// grenades on the stand-in Tier 1, under the method names the port covers.
if(ForceRequiredAddon("Weapon_Package_Tier1") == $Error::AddOn_NotFound)
{
   error("Stand-in Explosive 1 needs the stand-in Tier 1");
   return;
}

TT_defaultIfUnset("PlayerInfNades", 0);
TT_defaultIfUnset("StartfragNades", 4);

if(!isFile("add-ons/weapon_rocket_launcher/server.cs"))
{
   error("Stand-in Explosive 1 needs the base game's rocket launcher");
   return;
}

exec("./Item_GrenadeBag.cs");
exec("./Support_PrjLoop.cs");
exec("./Weapon_FragGrenade.cs");
exec("./Weapon_StickGrenade.cs");
exec("./Weapon_PetrBomb.cs");

TT_registerAmmoSet("tt_bag", "Grenade Bag", false);
TT_registerAmmoType("fragNades", "Conc", "Concs", true, "tt", "nades", "tt_bag");
TT_registerAmmoType("stickNades", "Stick", "Sticks", true, "tt", "nades", "tt_bag");
TT_registerAmmoType("molNades", "Firebomb", "Firebombs", true, "tt", "nades", "tt_bag");
