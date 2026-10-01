// Stand-in (CC0): each team function the port covers, in the shape its
// patterns read. Not the original's code.
function Slayer_TeamHandlerSG::pickTeam(%this, %client, %skipCurrentTeam)
{
	if(!%t.sort || %t.sortWeight <= 0 || (%t.numMembers["GameConnection"] >= %t.maxPlayers && %t.maxPlayers >= 0))
	continue;
	%ratio = %t.numMembers["GameConnection"] / %t.sortWeight;
	%r = getRandom(0, getFieldCount(%teams) - 1);
}

function Slayer_TeamHandlerSG::autoSort(%this, %client, %doNotRespawn)
{
	%team = %this.pickTeam(%client);
}

function Slayer_TeamHandlerSG::canDamage(%this, %teamA, %teamB)
{
	if(!%this.minigame.teams_friendlyFire && %teamA.isAlliedTeam(%teamB))
	return false;
}

function slayerTeamCmdBalance(%client, %mini, %clTeam, %a)
{
	if(!%mini.canEdit(%client))
	return;
	%mini.Teams.balanceTeams(1);
}

function slayerTeamCmdJoin(%client, %mini, %clTeam, %a)
{
	if(%mini.teams_lock && isObject(%clTeam))
	return;
	%team = %mini.Teams.getTeamFromName(%name);
	if(%clTeam.lock || %team.lock)
	return;
	if(%team.maxPlayers == 0)
	return;
}

function slayerTeamCmdLeave(%client, %mini, %clTeam, %a)
{
	if((%mini.teams_lock || %mini.teams_autoSort) && !%mini.canEdit(%client))
	return;
}

function slayerTeamCmdList(%client, %mini, %clTeam, %a)
{
	messageClient(%client, '', %team.getColorHex() @ %team.name);
}

function slayerTeamCmdCount(%client, %mini, %clTeam, %a)
{
	messageClient(%client, '', "(" @ %team.numMembers @ ")");
}

function slayerTeamCmdScore(%client, %mini, %clTeam, %a)
{
	messageClient(%client, '', %team.getScore());
}

function slayerTeamCmdListMembers(%client, %mini, %clTeam, %a)
{
	%cl = %team.member["GameConnection", %i];
	if(%team.numMembers["AiController"] > 1)
	messageClient(%client, '', %team.numMembers["AiController"] SPC "bots");
}

function serverCmdTeams(%client, %cmd, %a)
{
	%func = "slayerTeamCmd" @ %cmd;
}
