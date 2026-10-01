// Stand-in capture point functions (CC0): each in the shape the port's
// patterns read. Not the original's code.
function FxDtsBrick::createTrigger(%this, %data, %polyhedron)
{
	%boxDiff = vectorAdd(%boxDiff, "0 0 0.2");
}

function FxDtsBrick::setCPControl(%this, %color, %reset, %client)
{
	if(!%reset)
	%client.incScore(%mini.points_cp);
	%this.setColor(%color);
	if(%reset)
	%this.processInputEvent("onCPReset", %client);
	%this.processInputEvent("onCPCapture", %client);
	%this.processInputEvent("onCPCapture(Team" @ %i + 1 @ ")", %client);
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
	else
	%brick.setCPControl(%attCol, 0, %client);
	}}
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
	case "TeamSpawn" or "TeamVehicle":
	%this.controlColor = %color;
	case "CP":
	%this.setCPControl(%color, 0, %client);
}

function FxDtsBrick::setTeamControlLocked(%this, %mode, %color, %flag, %client)
{
	%this.isLocked[%color] = %flag;
	if(%color != %brColor)
	%this.capture[%color] = 0;
}

function fxDTSBrick::checkTeam(%this, %type, %string, %eventnums, %client)
{
	%pass = %team.isAlliedTeam(%checkTeam);
	if(getWordCount(%eventnums) != 2)
	%this.onTeamCheckTrue(%client);
}

function fxDTSBrick::checkTeamCount(%this, %string, %operator, %check, %eventnums, %client)
{
	case 0: %pass = %checkteam.numMembers >= %check;
	case 1: %pass = %checkteam.numMembers <= %check;
	case 2: %pass = %checkteam.numMembers == %check;
	case 3: %pass = %checkteam.numMembers != %check;
}

$Slayer::Server::Events::RestrictedEvent__["Minigame", "BottomPrintAll"] = 2;
$Slayer::Server::Events::RestrictedEvent__["Minigame", "CenterPrintAll"] = 2;
$Slayer::Server::Events::RestrictedEvent__["Minigame", "ChatMsgAll"] = -1;
$Slayer::Server::Events::RestrictedEvent__["Minigame", "incTimeRemaining"] = -1;
$Slayer::Server::Events::RestrictedEvent__["Minigame", "Reset"] = -1;
$Slayer::Server::Events::RestrictedEvent__["Minigame", "RespawnAll"] = -1;
$Slayer::Server::Events::RestrictedEvent__["Minigame", "setTimeRemaining"] = -1;
$Slayer::Server::Events::RestrictedEvent__["Minigame", "Win"] = -1;
$Slayer::Server::Events::RestrictedEvent__["Slayer_TeamSO", "BottomPrintAll"] = -1;
$Slayer::Server::Events::RestrictedEvent__["Slayer_TeamSO", "CenterPrintAll"] = -1;
$Slayer::Server::Events::RestrictedEvent__["Slayer_TeamSO", "ChatMsgAll"] = -1;
$Slayer::Server::Events::RestrictedEvent__["Slayer_TeamSO", "RespawnAll"] = -1;
$Slayer::Server::Events::RestrictedEvent__["Slayer_TeamSO", "IncScore"] = -1;

package Slayer_FxDtsBrick
{
	function serverCmdAddEvent(%client, %enabled, %inputEventIdx, %delay, %targetIdx, %namedTargetNameIdx, %outputEventIdx, %par1, %par2, %par3, %par4)
	{
		if(isSlayerMinigame(%mini) && %mini.restrictOutputEvents)
		{
			if(!%mini.canEdit(%client, $Slayer::Server::Events::RestrictedEvent__[%target, %outputEvent]))
				return;
		}
	}

	function FxDtsBrick::onPlayerTouch(%this, %player)
	{
		%team = %client.getTeam();
		%this.processInputEvent("onPlayerTouch(Team" @ %team.getGroup().indexOf(%team) + 1 @ ")", %client);
	}

	function FxDtsBrick::onActivate(%this, %player, %client)
	{
		%team = %client.getTeam();
		%this.processInputEvent("onActivate(Team" @ %team.getGroup().indexOf(%team) + 1 @ ")", %client);
	}

	function Slayer::createBrickEvents(%this)
	{
		for(%i=1; %i <= $Pref::Slayer::Server::Teams::maxEvents; %i ++)
		{
			registerInputEvent(FxDtsBrick, "onPlayerTouch(Team" @ %i @ ")", "Self FxDtsBrick\tPlayer Player\tClient GameConnection\tMiniGame MiniGame");
			registerInputEvent(FxDtsBrick, "onActivate(Team" @ %i @ ")", "Self FxDtsBrick\tPlayer Player\tClient GameConnection\tMiniGame MiniGame");
		}
		registerMultiSourceInputEvent(FxDtsBrick, "onMinigameDeath",
			"Self FxDtsBrick\tClient GameConnection\tPlayer(Killer) Player\tClient(Killer) GameConnection\tMiniGame MiniGame");
		registerMultiSourceInputEvent(FxDtsBrick, "onMinigameJoin", "Self FxDtsBrick\tPlayer Player\tClient GameConnection\tMiniGame MiniGame");
		registerMultiSourceInputEvent(FxDtsBrick, "onMinigameLeave", "Self FxDtsBrick\tPlayer Player\tClient GameConnection\tMiniGame MiniGame");
		registerMultiSourceInputEvent(FxDtsBrick, "onMinigameRoundStart", "Self FxDtsBrick\tMiniGame MiniGame");
		registerMultiSourceInputEvent(FxDtsBrick, "onMinigameRoundEnd", "Self FxDtsBrick\tMiniGame MiniGame");
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
		registerEventTarget("Team(Client) Slayer_TeamSO", "GameConnection", "%client.slyrTeam");
		registerEventTarget("Team(Brick) Slayer_TeamSO", "FxDtsBrick", "%this.getTeamControlList()");
		registerOutputEvent(Slayer_TeamSO, "BottomPrintAll", "string 150 120" TAB "int 1 8 2" TAB "bool 0", 1);
		registerOutputEvent(Slayer_TeamSO, "CenterPrintAll", "string 150 120" TAB "int 1 8 2", 1);
		registerOutputEvent(Slayer_TeamSO, "ChatMsgAll", "string 150 130", 1);
		registerOutputEvent(Slayer_TeamSO, "RespawnAll", "", 1);
		registerOutputEvent(Slayer_TeamSO, "IncScore", "int -9999 9999 2", 1);
	}
};
