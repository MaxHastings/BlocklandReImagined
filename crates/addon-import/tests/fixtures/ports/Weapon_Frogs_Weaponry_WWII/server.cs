// Stand-in for the Weapon_Frogs_Weaponry_WWII port tests (CC0): one gun of
// our own on the stand-in Frog's Weaponry and Tier 1.
%error = ForceRequiredAddOn("Weapon_Frogs_Weaponry");
if(%error != $Error::AddOn_NotFound)
	exec("./Weapon_Thompson.cs");
