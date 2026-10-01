// Stand-in for Tier+Tactical Melee Extended II: our own datablocks, numbers
// and sounds, laid out as the original is so its port applies.
function TT_defaultIfUnset(%pref, %default, %category)
{
	if($Pref::Server["::" @ %category @ "::" @ %pref] $= "")
	$Pref::Server["::" @ %category @ "::" @ %pref] = %default;
}

if(ForceRequiredAddOn("Weapon_Melee_Extended") == $Error::AddOn_NotFound)
{
   error("ERROR: stand-in Melee Extended II needs Melee Extended");
}
else
{
   exec("./Weapons.cs");
   if($Pref::Server::TT::EasterEgg)
      exec("./Hidden.cs");
   exec("./Shield.cs");
}
