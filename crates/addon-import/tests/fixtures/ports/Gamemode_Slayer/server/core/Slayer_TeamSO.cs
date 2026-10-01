// Stand-in team functions (CC0): each in the shape the port's patterns
// read. Not the original's code.
function Slayer_TeamSO::onMinigameReset(%this)
{
	if(%this.minigame.points > 0)
		%this.setArtificialScore(0);
}

function Slayer_TeamSO::chatMsgAll(%this, %msg, %client)
{
	if(isObject(%client))
		%msg = strReplace(%msg, "%1", %client.getPlayerName());
	%this.messageAll('', %msg);
}

function Slayer_TeamSO::respawnAll(%this, %client)
{
	for(%i = %this.numMembers - 1; %i >= 0; %i --)
		%this.member[%i].spawnPlayer();
}

function Slayer_TeamSO::setArtificialScore(%this, %flag)
{
	%this.score = %flag;
	%mini = %this.minigame;
	%winner = %mini.victoryCheck_Points();
	return %flag;
}

function Slayer_TeamSO::getScore(%this)
{
	%score = %this.getArtificialScore();
	return %score;
}

function Slayer_TeamSO::updateRespawnTime(%this, %type, %flag, %old)
{
	%time = %flag * 1000;
	if(%flag != -1)
		%time = mClampF(%time, 1000, 999999);
}
