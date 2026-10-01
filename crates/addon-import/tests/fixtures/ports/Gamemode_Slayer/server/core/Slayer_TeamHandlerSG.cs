// Stand-in (CC0): each team function the port covers, in the shape its
// patterns read. Not the original's code.
function Slayer_TeamHandlerSG::pickTeam(%this, %client, %skipCurrentTeam)
{
	%ratio = %t.numMembers["GameConnection"] / %t.sortWeight;
	%r = getRandom(0, getFieldCount(%teams) - 1);
	return getField(%teams, %r);
}

function Slayer_TeamHandlerSG::autoSort(%this, %client, %doNotRespawn)
{
	%team = %this.pickTeam(%client);
	%team.addMember(%client, "", %doNotRespawn);
}

function Slayer_TeamHandlerSG::canDamage(%this, %teamA, %teamB)
{
	if(!%this.minigame.teams_friendlyFire && %teamA.isAlliedTeam(%teamB))
		return false;
	return true;
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
}

function serverCmdTeams(%client, %cmd, %a)
{
	%func = "slayerTeamCmd" @ %cmd;
	call(%func, %client, %mini, %clTeam, %a);
}
