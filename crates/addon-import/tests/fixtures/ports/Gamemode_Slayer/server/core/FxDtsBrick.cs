// Stand-in capture point functions (CC0): each in the shape the port's
// patterns read. Not the original's code.
function FxDtsBrick::createTrigger(%this, %data, %polyhedron)
{
	%boxDiff = vectorSub(%boxMax, %boxMin);
	%boxDiff = vectorAdd(%boxDiff, "0 0 0.2");
	return %trigger;
}

function FxDtsBrick::setCPControl(%this, %color, %reset, %client)
{
	if(!%reset)
		%client.incScore(%mini.points_cp);
	%this.setColor(%color);
	if(%reset)
		%this.processInputEvent("onCPReset", %client);
	else
	{
		%this.processInputEvent("onCPCapture", %client);
		%this.processInputEvent("onCPCapture(Team" @ %i + 1 @ ")", %client);
	}
}

function Slayer_CPTriggerData::onTickTrigger(%this, %trigger, %player)
{
	if(%brick.capture[%attCol] < %maxTicks)
	{
		if(%brick.isLocked[%attCol])
			%client.bottomPrint("<just:center>\c5Locked for now.", 1);
		%trigger.decreaseTimer[%attCol] = %this.scheduleNoQuota(%this.tickPeriodMS + 1000, decreaseCapture, %trigger, %attCol);
		%brick.capture[%attCol] ++;
		%client.bottomPrint("<just:center>" @ %a @ %d, 1, true);
		if(%mini.CPTransitionColors)
		{
			%mix = Slayer_Support::getAverageColor(%rgb, %team.colorRGB);
			%brick.setColor(Slayer_Support::getClosestPaintColor(%mix));
		}
	}
	else
		%brick.setCPControl(%attCol, 0, %client);
}

function Slayer_CPTriggerData::decreaseCapture(%this, %trigger, %color)
{
	if(%cl.slyrTeam.color == %color) return;
	%brick.capture[%color] --;
	if(%brick.capture[%color] <= 0) %brick.setColor(%defCol);
	else %trigger.decreaseTimer[%color] = %this.scheduleNoQuota(%this.tickPeriodMS, decreaseCapture, %trigger, %color);
}

function FxDtsBrick::setTeamControl(%this, %color, %client)
{
	switch$(%db.slyrType)
	{
		case "TeamSpawn" or "TeamVehicle":
			%this.controlColor = %color;
		case "CP":
			%this.setCPControl(%color, 0, %client);
	}
}

function FxDtsBrick::setTeamControlLocked(%this, %mode, %color, %flag, %client)
{
	if(%this.isLocked[%color] != %flag)
	{
		%this.isLocked[%color] = %flag;
		if(%color != %brColor)
			%this.capture[%color] = 0;
	}
}

function fxDTSBrick::checkTeam(%this, %type, %string, %eventnums, %client)
{
	%pass = %team.isAlliedTeam(%checkTeam);
	if(getWordCount(%eventnums) != 2)
		%this.onTeamCheckTrue(%client);
}

function fxDTSBrick::checkTeamCount(%this, %string, %operator, %check, %eventnums, %client)
{
	switch(%operator)
	{
		case 0: %pass = %checkteam.numMembers >= %check;
		case 1: %pass = %checkteam.numMembers <= %check;
		case 2: %pass = %checkteam.numMembers == %check;
		case 3: %pass = %checkteam.numMembers != %check;
	}
}

package Slayer_FxDtsBrick
{
	function Slayer::createBrickEvents(%this)
	{
		registerInputEvent("fxDTSBrick", "onTeamCheckTrue", "Self fxDTSBrick" TAB "Player Player" TAB "Client GameConnection" TAB "MiniGame MiniGame" TAB "OwnerPlayer Player" TAB "OwnerClient GameConnection", 1);
		registerOutputEvent(FxDtsBrick, "setTeamControl", "paintColor 2", 1);
		registerOutputEvent(FxDtsBrick, "setTeamControlLocked", "list Mine 0 Colour 1 Every 2" TAB "paintColor 1" TAB "bool", 1);
		registerOutputEvent(GameConnection, "addLives", "int 0 50 2");
		registerOutputEvent(GameConnection, "addDeaths", "int 0 50 2");
		registerOutputEvent(GameConnection, "addKills", "int 0 50 2");
		registerOutputEvent(GameConnection, "setLives", "int 0 50 2");
		registerOutputEvent(GameConnection, "setDeaths", "int 0 50 2");
		registerOutputEvent(GameConnection, "setKills", "int 0 50 2");
		registerOutputEvent(GameConnection, "joinTeam", "string 40 60" TAB "string 40 60" TAB "bool 0");
		registerOutputEvent(Minigame, "incTimeRemaining", "int -99 99 2" TAB "bool 1");
		registerOutputEvent(Minigame, "setTimeRemaining", "int 1 99 2" TAB "bool 1");
		registerOutputEvent(Minigame, "Win", "list Me 0 MyTeam 1 Player 2 Team 3 Text 4 Nobody 5" TAB "string 60 90", 1);
		registerOutputEvent("fxDTSBrick", "checkTeam", "list In 0 NotIn 1 Ally 2" TAB "string 40 70" TAB "string 6 20", 1);
		registerOutputEvent("fxDTSBrick", "checkTeamCount", "string 40 70" TAB "list >= 0 <= 1 == 2 != 3" TAB "string 6 20" TAB "string 6 20", 1);
	}
};
