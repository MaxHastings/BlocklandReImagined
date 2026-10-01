// Stand-in (CC0): the spawn choice's shape the port reads.
function Slayer_MiniGameSO::pickSpawnPoint(%this, %client)
{
	%team = %client.getTeam();
	for(%i = 0; %i < %numSpawns; %i ++)
	{
		%sp = Slayer.Spawns.getObject(%i);
		%type = %sp.getDatablock().slyrType;
		%color = %sp.getTeamControl();
		if(%type !$= "TeamSpawn" || %color != %team.color)
			continue;
		if(!%sp.isBlocked())
			%open ++;
	}
	if(%db.getName() !$= "brickSpawnPointData")
		return;
}

function Slayer_MiniGameSO::victoryCheck_Lives(%this)
{
	if(!%t.isTeamDead())
	{
	if(%this.teams_allySameColors && %t.color == %winnerColor)
	%winner = %t;
	if(%count == 1)
	return %winner;
	}
}

function Slayer_MiniGameSO::victoryCheck_Points(%this)
{
	if(%t.getScore() >= %this.points)
	%winner = %t;
	if(%cl.score >= %this.points)
	%winner = %cl;
}

function Slayer_MiniGameSO::victoryCheck_Time(%this, %ticks)
{
	%time = %this.time * 60000;
	if(%t.winOnTimeUp)
	%winOnTimeUp = 1;
	if(%sc == %least && %least > 0)
	%count ++;
	%this.messageAll('', '\c3%1 \c5remaining.', %remain);
}

function Slayer_MiniGameSO::preRoundCountdownTick(%this, %ticks)
{
	%remain = %this.preRoundSeconds - %ticks;
	%this.play2dAll(Slayer_Begin_Sound);
	%this.centerPrintAll("GO!", 2);
	%sound = "Slayer_" @ %remain @ "_Seconds_Sound";
	%cl.player.changeDatablock(PlayerFrozenArmor);
	%ai.player.stopHoleLoop();
}

function Slayer_MiniGameSO::startRound(%this)
{
	if(%cl.player.getDatablock().getID() == PlayerFrozenArmor.getID())
	{
		%db = %this.playerDatablock;
		%cl.player.changeDatablock(%db);
	}
	%ai.player.resetHoleLoop();
	$InputTarget_["MiniGame"] = %this;
	processMultiSourceInputEvent("onMinigameRoundStart", 0, %this);
}

function Slayer_MiniGameSO::endRound(%this, %winner, %resetTime)
{
	if(getField(%winner, 0) $= "CUSTOM")
		%nameList = getField(%winner, 1);
	if(%count > 1)
		%msg = '<color:ff00ff>%1 tied this round.';
	else
		%msg = '<color:ffff00>%1 \c5won this round. Resetting in %4 seconds.';
	%cl.setDead(true);
	if(!%this.allowMoveWhileResetting)
		%cl.camera.setMode(corpse, %winner.player);
	%ai.setDead(true);
	if(!%this.allowMoveWhileResetting)
		%ai.player.stopHoleLoop();
	%resetTime = %this.timeBetweenRounds * 1000;
	%msg = '\c5Nobody won this round. Resetting in %4 seconds.';
	messageClient(%cl, '', "\c3No \"End of Round Report\" without the client.");
	if(%this.eorrEnable)
		%this.sendScoreListAll();
	%this.bottomPrintAll("Resetting in" SPC %timeLeft, 2, 1);
	$InputTarget_["MiniGame"] = %this;
	processMultiSourceInputEvent("onMinigameRoundEnd", 0, %this);
}

function Slayer_MiniGameSO::sendScoreListAll(%this)
{
	%var3 = "Score";
	%var4 = "Kills";
	%var5 = "Deaths";
	%var6 = "Rounds Won";
	if(%this.eorrDisplayVictory)
	%victoryStatus = 1;
	%var1 = (%cl.roundWon ? "VICTORY" : "DEFEAT");
	if(%this.gameMode.template.useTeams && %this.Teams.getCount() > 0 && %this.eorrDisplayTeamScores)
	%this.commandToAllSlayerClients('Slayer_ctrDisplayAdd', "<b>Teams:</b><br>");
	%teamList = %this.Teams.getTeamListSortedScore();
	%var4 = %t.getKills();
	%var5 = %t.getDeaths();
	%var6 = %t.wins;
	%this.commandToAllSlayerClients('Slayer_ctrDisplayAdd', "<br><b>Players:</b><br>");
	%memberList = %this.getMemberListSortedScore();
	%var6 = (%team > 0 ? "" : %cl.wins);
}

function Slayer_MiniGameSO::incTimeRemaining(%this, %flag, %display)
{
	if(%rmndr >= 15000)
	%remain = %this.timeRemaining + (15000 - %rmndr);
	if(%display)
	%this.messageAll('', "\c5Extended by\c3" SPC %flag SPC "\c5" @ %min @ ".");
}

function Slayer_MiniGameSO::setTimeRemaining(%this, %flag, %display)
{
	if(%display)
	%this.messageAll('', "\c5Time now\c3" SPC %flag SPC "\c5" @ %min @ ".");
}

function MiniGameSO::Win(%this, %mode, %flag, %client)
{
	case 0: %this.endRound(%client);
	case 1: %this.endRound(%team);
	case 2: %cl = findClientByName(%flag);
	case 3: %team = %this.Teams.getTeamFromName(%flag);
	case 4: %this.endRound("CUSTOM" TAB %flag);
}

function serverCmdSlayer(%client, %cmd)
{
	messageClient(%client, '', '\c5 + %1 - The last %2 standing wins.', %lives, %person);
	messageClient(%client, '', "\c5 +" SPC %mini.customRule);
}

function Slayer_MiniGameSO::resetCapturePoints(%this, %client)
{
	%cp.setCPControl(%cp.origColor, 1, %client);
}

package Slayer_MiniGameSO
{
	function Slayer_MiniGameSO::addMember(%this, %client)
	{
		if(%class $= "AiController")
		{
			%client.removeAllObjectives();
			%client.assignObjectives();
			%client.spawnPlayer();
		}
		$InputTarget_["Client"] = %client;
		processMultiSourceInputEvent("onMinigameJoin", %client, %this);
	}

	function Slayer_MiniGameSO::removeMember(%this, %client)
	{
		$InputTarget_["Client"] = %client;
		processMultiSourceInputEvent("onMinigameLeave", %client, %this);
		%winner = %this.victoryCheck_Lives();
	}

	function Slayer_MiniGameSO::Reset(%this, %client)
	{
	%ai.setDead(0);
	%ai.setLives(%this.lives);
	%cl.setLives((isObject(%t) && %t.lives >= 0) ? %t.lives : %this.lives);
	%cl.setKills(0);
	%cl.setDeaths(0);
}
	function Slayer_MiniGameSO::endGame(%this)
	{
	for(%i = %this.numMembers["AiController"] - 1; %i >= 0; %i --)
	%this.member["AiController", %i].delete();
}
};
activatePackage(Slayer_MiniGameSO);

// Stand-ins (CC0) for the bot shapes the port reads.
function Slayer_MiniGameSO::addBotToGame(%this)
{
	class = Slayer_AiController;
	hName = "Bot" SPC getRandomFirstName();
}

function Slayer_MiniGameSO::canDamage(%this, %objA, %classA, %objB, %classB)
{
	if(%classA $= "AiPlayer")
	return %this.botDamage;
}

function Slayer_MiniGameSO::updateRespawnTime(%this, %type, %flag, %old)
{
	case "bot": %this.botRespawnTime = %time;
}
