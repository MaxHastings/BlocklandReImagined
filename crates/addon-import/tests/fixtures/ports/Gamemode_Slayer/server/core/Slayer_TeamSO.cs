// Stand-in team functions (CC0): each in the shape the port's patterns
// read. Not the original's code.
function Slayer_TeamSO::onMinigameReset(%this)
{
	if(%this.minigame.points > 0)
		%this.setArtificialScore(0);
	%mini = %this.minigame;
	if(%mini.gameMode.template.useTeams && !%mini.teams_shuffleTeams)
		%this.botFillTeam();
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

function Slayer_TeamSO::onRemove(%this)
{
	%this.deleteAllBots();
}

function Slayer_TeamSO::addMember(%this, %client, %reason, %doNotRespawn)
{
	%class = %client.getClassName();
	if(%class $= "GameConnection")
	{
		//HUMAN - ANNOUNCEMENT
		messageClient(%client, '', "\c5Joined");
	}
}

function Slayer_TeamSO::onAddMember(%this, %client)
{
	if(%this.botFillLimit > 0)
		%this.botFillTeam();
}

function Slayer_TeamSO::onRemoveMember(%this, %client)
{
	if(%this.botFillLimit > 0)
		%this.botFillTeam();
}

function Slayer_TeamSO::botFillTeam(%this)
{
	%limit = %this.botFillLimit;
	while(%this.numMembers > %limit && %this.numMembers["AiController"] > 0)
		%this.member["AiController", 0].delete();
	while(%this.numMembers < %limit)
		%this.addMember(%this.minigame.addBotToGame());
}

function Slayer_TeamSO::updateBotFillLimit(%this, %flag)
{
	if(%flag < 1 || ($AddOnLoaded__["Bot_Hole"] && $AddOnLoaded__["Bot_Blockhead"]))
		%this.botFillTeam();
	else
		%this.botFillLimit = -1;
}
