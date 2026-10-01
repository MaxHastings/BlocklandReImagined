// Stand-in preferences (CC0): the original's layout, this stand-in's values.
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 3;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.CTF_flagReturnsToWin";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.CTF_flagReturnOnlyAtReturnBrick";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.CTF_returnWithoutOwn";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.CTF_neutralFlags";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 25;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.CTF_points_Flag";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 5;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.CTF_points_FlagRecovery";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.CTF_flagRecovery";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 7;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.CTF_flagDroppedRespawnTime";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.CTF_flagReturnedRespawnTime";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.CTF_manualFlagDrop";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.CTF_requireEnemyPlayers";
};

registerOutputEvent(Player, "DropFlag", "");
