// Stand-in preferences (CC0): the original's layout, this stand-in's values.
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 6;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.timeBetweenRounds";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.preRoundSeconds";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.allowMoveWhileResetting";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.lives";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.points";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.time";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.teams_autoSort";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.teams_balanceOnNew";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.teams_notifyMemberChanges";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.teams_friendlyFire";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.teams_lock";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.teams_allySameColors";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = false;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.teams_allowCustomFaces";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 7;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.points_CP";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 100;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "$Pref::Slayer::Server::CPTriggerTickMS";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.CPTransitionColors";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 6;
	permissionLevel = $Slayer::PermissionLevel["Host"];
	variable = "$Pref::Slayer::Server::Teams::maxEvents";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.teams_teamOnlyDeadCam";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.spectateAutoCam";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.spectateCapturePoints";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.clearStats";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Host"];
	variable = "%mini.restrictOutputEvents";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.eorrEnable";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.eorrDisplayTeamScores";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.eorrDisplayVictory";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 3;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.points_killBot";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.botDamage";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 2;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.respawnTime_bot";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 2;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.chat_deadChatMode";
	list_items = "0 Disabled" NL "1 Enabled" NL "2 Enabled - Global Only" NL "3 Enabled - Team Only";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 2;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.deathMsgMode";
	list_items = "0 Do Not Display" NL "1 Colored Names" NL "2 Non-Colored Names";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.chat_enableTeamChat";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 3;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.chat_teamDisplayMode";
	list_items = "0 Disabled" NL "1 Color Tag" NL "2 Color Name" NL "3 Add Name to Tag";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = false;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.disableSlayerMessages";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "$Pref::Slayer::Server::AutoStartMiniGame";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.clearScores";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 5;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.colorIdx";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.isDefaultMinigame";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = "STAND-IN HOST";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.hostName";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 2;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.lateJoinTime";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.resetOnEmpty";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 7;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.spawnBrickColor";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 3;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "$Pref::Slayer::Server::MiniGameCreationRights";
	list_items = "0 Host" NL "1 Super Admin" NL "2 Admin" NL "3 Everyone";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 3;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.editRights";
	list_items = "3 Creator" NL "4 Full Trust" NL "5 Build Trust" NL "0 Host" NL "1 Super Admin" NL "2 Admin";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.leaveRights";
	list_items = "3 Creator" NL "4 Full Trust" NL "5 Build Trust" NL "0 Host" NL "1 Super Admin" NL "2 Admin" NL "6 Everyone";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 3;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.resetRights";
	list_items = "3 Creator" NL "4 Full Trust" NL "5 Build Trust" NL "0 Host" NL "1 Super Admin" NL "2 Admin";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = true;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.enablePlayerLights";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = true;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.enablePlayerSuicide";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 90;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.nameDistance";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = -3;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.points_friendlyFire";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = false;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.friendlyFireProgressivePenalty";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 3;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.respawnPenalty_FriendlyFire";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = "Stand-in announcement";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.announcements";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = true;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "$Pref::Slayer::Server::SavePrefsOnUpdate";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.teams_balanceTeams";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.teams_punishFF";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = true;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.friendlyFireDisplayWarning";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 2;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.teams_roundsBetweenShuffles";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.teams_shuffleTeams";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.teams_shuffleMode";
	list_items = "0 Random" NL "1 New Team Every Time";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.teams_swapMode";
	list_items = "0 Regulated" NL "1 Unregulated (always join)";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 12;
	variable = "$Pref::Server::Slayer::Teams::maxTeams";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = "Stand-in Slayer";
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.Title";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 4;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.respawnTime_player";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 12;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.respawnTime_vehicle";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 25;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.respawnTime_brick";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 1;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.enableWand";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = nameToID(SwordItem);
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.startEquip0";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = nameToID(HammerItem);
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.startEquip1";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = nameToID(WrenchItem);
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.startEquip2";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.startEquip3";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 0;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.startEquip4";
};
