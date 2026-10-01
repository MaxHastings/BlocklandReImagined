// CC0 stand-in for Kai's Event_AddAmmoTT, written for the port tests: it
// keeps the names and shapes the port reads, in our own code.
if(forceRequiredAddOn("Weapon_Package_Tier1") == $Error::AddOn_NotFound)
	error("Event_AddAmmoTT stand-in: Tier 1 is missing");
else
	exec("./Event_AddAmmoTT.cs");
