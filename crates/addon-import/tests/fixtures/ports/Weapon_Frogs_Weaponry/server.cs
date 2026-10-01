// Stand-in for the Weapon_Frogs_Weaponry port tests (CC0): two of our own
// guns on the stand-in Tier 1, under the datablock and method names the
// port reads.
if(ForceRequiredAddOn("Weapon_Package_Tier1") == $Error::AddOn_NotFound)
	return;

RTB_registerPref("Spinner can deploy","Stand-in | Guns","$Pref::Server::FW::VulcanDeploy","bool","Weapon_Frogs_Weaponry",1,0,1);
RTB_registerPref("Spinner slows","Stand-in | Guns","$Pref::Server::FW::VulcanSlow","bool","Weapon_Frogs_Weaponry",1,0,1);
RTB_registerPref("Launcher slows","Stand-in | Guns","$Pref::Server::FW::PayloadSlow","bool","Weapon_Frogs_Weaponry",1,0,1);

TT_registerAmmoType("50Cal", "50", "Spinner belts", false, "fw", "weps", "fw_cache");
TT_registerAmmoType("Payload", "P", "Launcher shells", false, "fw", "weps", "fw_cache");

// Bodies laid over the player and lifted again.
function Player::pushDatablock(%this,%data)
{
	if(fileName(%this.dataBlock.shapeFile) !$= fileName(%data.shapeFile))
	return;
}

function Player::popDatablock(%this,%data)
{
	%this.setDatablock(%this.altData[%this.altDataNum - 1]);
}

function Armor::onTrigger(%this,%obj,%slot,%val)
{
	if(isObject(%image = %obj.getMountedImage(0)))
		%image.onAltTrigger(%obj,%this,%slot,%val);
	Parent::onTrigger(%this,%obj,%slot,%val);
}

datablock PlayerData(DeployedArmor : PlayerStandardArmor)
{
   maxForwardSpeed = 0;
   maxBackwardSpeed = 0;
   maxSideSpeed = 0;
   canJet = false;
   uiName = "";
};

datablock PlayerData(SlowedArmor : PlayerStandardArmor)
{
   maxForwardSpeed = 4;
   maxBackwardSpeed = 3;
   maxSideSpeed = 3;
   uiName = "";
};

exec("./Weapon_Vulcan.cs");
exec("./Weapon_PayloadLauncher.cs");
