// Stand-in bonus kills (CC0): the original's layout, this stand-in's values.
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = true;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.bKills_enable";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 3;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.bKills_kSpree";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 2;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.bKills_pointsKillSpree";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 3;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.bKills_pointsMultiKill";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = true;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.bKills_enable";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 3;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.bKills_kSpree";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 2;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.bKills_pointsKillSpree";
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	defaultValue = 3;
	permissionLevel = $Slayer::PermissionLevel["Any"];
	variable = "%mini.bKills_pointsMultiKill";
};

$Slayer::Server::BK::KillSpree[0] = "\c3(On a roll | %1)";
$Slayer::Server::BK::KillSpree[100] = "\c3(A hundred down | %1)";
$Slayer::Server::BK::KillSpree[200] = "\c3(Two hundred down | %1)";

$Slayer::Server::BK::QuickKillTimeOut = 1500;
$Slayer::Server::BK::QuickKill[0] = "\c3(Quick x%1)";
$Slayer::Server::BK::QuickKill[2] = "\c3(Quick pair)";
$Slayer::Server::BK::QuickKill[3] = "\c3(Quick three)";
$Slayer::Server::BK::QuickKill[4] = "\c3(Quick four)";
$Slayer::Server::BK::QuickKill[5] = "\c3(Quick five)";
$Slayer::Server::BK::QuickKill[6] = "\c3(Quick six)";
$Slayer::Server::BK::QuickKill[7] = "\c3(Quick seven)";
$Slayer::Server::BK::QuickKill[8] = "\c3(Quick eight)";
$Slayer::Server::BK::QuickKill[9] = "\c3(Quick nine)";
$Slayer::Server::BK::QuickKill[10] = "\c3(Quick ten)";
