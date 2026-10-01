// Stand-in team preferences (CC0): the original's layout, this stand-in's
// values.
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = -1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "lives";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "sort";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "sortWeight";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = -1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "maxPlayers";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "lock";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "winOnTimeUp";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "playerScale";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = nameToID(playerStandardArmor);
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "playerDatablock";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = -1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "respawnTime_Player";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = nameToID(hammerItem);
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "startEquip0";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = nameToID(wrenchItem);
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "startEquip1";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = nameToID(printGun);
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "startEquip2";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "startEquip3";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "startEquip4";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "syncLoadout";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 3;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uniform";
	list_items = "0 NONE" NL "1 Shirt Only" NL "2 Full" NL "3 Custom";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "1";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_accent";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "TEAMCOLOR";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_accentColor";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "1";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_chest";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "Alyx";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_decalName";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "smileyEvil1";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_faceName";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "1";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_hat";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "TEAMCOLOR";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_hatColor";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "0.5 0.25 0 1";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_headColor";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "0";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_hip";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "0 0 1 1";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_hipColor";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "1";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_lArm";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "TEAMCOLOR";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_lArmColor";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "1";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_lHand";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "0.5 0.25 0 1";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_lHandColor";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "1";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_lLeg";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "TEAMCOLOR";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_lLegColor";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "2";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_pack";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "TEAMCOLOR";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_packColor";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "0";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_rArm";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "TEAMCOLOR";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_rArmColor";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "0";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_rHand";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "0.5 0.25 0 1";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_rHandColor";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "0";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_rLeg";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "TEAMCOLOR";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_rLegColor";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "1";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_secondPack";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "1 1 0 1";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_secondPackColor";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	category = "Uniform";
	defaultValue = "TEAMCOLOR";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "uni_torsoColor";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = -1;
	permissionLevel = $Slayer::PermissionLevel["Admin"];
	variable = "botFillLimit";
};
new ScriptObject(Slayer_TeamPrefSO : Slayer_DefaultTeamPrefSO)
{
	defaultValue = nameToID(PlayerStandardArmor);
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "captain_playerDatablock";
};
new ScriptObject(Slayer_TeamPrefSO : Slayer_DefaultTeamPrefSO)
{
	defaultValue = nameToID(SwordItem);
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "captain_startEquip0";
};
new ScriptObject(Slayer_TeamPrefSO : Slayer_DefaultTeamPrefSO)
{
	defaultValue = nameToID(HammerItem);
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "captain_startEquip1";
};
new ScriptObject(Slayer_TeamPrefSO : Slayer_DefaultTeamPrefSO)
{
	defaultValue = nameToID(WrenchItem);
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "captain_startEquip2";
};
new ScriptObject(Slayer_TeamPrefSO : Slayer_DefaultTeamPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "captain_startEquip3";
};
new ScriptObject(Slayer_TeamPrefSO : Slayer_DefaultTeamPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "captain_startEquip4";
};
new ScriptObject(Slayer_TeamPrefSO : Slayer_DefaultTeamPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "enableTeamChat";
};
new ScriptObject(Slayer_TeamPrefSO : Slayer_DefaultTeamPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "hideKillMsgs";
};
new ScriptObject(Slayer_TeamPrefSO : Slayer_DefaultTeamPrefSO)
{
	defaultValue = "00ff00";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "chatColor";
};
new ScriptObject(Slayer_TeamPrefSO : Slayer_DefaultTeamPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "spectate";
};
