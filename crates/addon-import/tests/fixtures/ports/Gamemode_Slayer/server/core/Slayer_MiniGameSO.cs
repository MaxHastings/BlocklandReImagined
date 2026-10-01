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
	}
	if(%count == 1)
		return %winner;
	return -1;
}

function Slayer_MiniGameSO::victoryCheck_Points(%this)
{
	if(%t.getScore() >= %this.points)
		%winner = %t;
	if(%cl.score >= %this.points)
		%winner = %cl;
	return %winner;
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
}

function Slayer_MiniGameSO::startRound(%this)
{
	if(%cl.player.getDatablock().getID() == PlayerFrozenArmor.getID())
	{
		%db = %this.playerDatablock;
		%cl.player.changeDatablock(%db);
	}
}

function Slayer_MiniGameSO::endRound(%this, %winner, %resetTime)
{
	%cl.setDead(true);
	if(!%this.allowMoveWhileResetting)
		%cl.camera.setMode(corpse, %winner.player);
	%resetTime = %this.timeBetweenRounds * 1000;
	%msg = '\c5Nobody won this round. Resetting in %4 seconds.';
	%this.bottomPrintAll("Resetting in" SPC %timeLeft, 2, 1);
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
	function Slayer_MiniGameSO::removeMember(%this, %client)
	{
		%winner = %this.victoryCheck_Lives();
	}

	function Slayer_MiniGameSO::Reset(%this, %client)
	{
		%cl.setLives((isObject(%t) && %t.lives >= 0) ? %t.lives : %this.lives);
	}
};
activatePackage(Slayer_MiniGameSO);
