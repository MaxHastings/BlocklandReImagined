// Stand-in (CC0): the death and score shapes the port reads. Not the
// original's code.
package Slayer_GameConnection
{
	function GameConnection::onDeath(%this, %obj, %killer, %type, %area)
	{
		%this.addDeaths(1);
		if(%killer != %this && isFunction(%killerClass, addKills))
			%killer.addKills(1);
		if(%this.getLives() > 0)
		{
			%this.addLives(-1);
			if(%this.getLives() <= 0)
				%this.setDead(1);
		}
		%this.centerPrint("\c5You have \c30 \c5lives left.", 3);
		$InputTarget_["Client(Killer)"] = %killer;
		processMultiSourceInputEvent("onMinigameDeath", %this, %mini);
	}

	function GameConnection::setScore(%this, %flag)
	{
		%winner = %mini.victoryCheck_Points();
		%this.spectateStartSched = %this.scheduleNoQuota(3000, "spectateInit");
	}

	function Observer::onTrigger(%this, %camera, %button, %state)
	{
		if(getSimTime() - %client.lastDeathTime < 500)
			return;
		if(%button == 0 && %camera.isOrbitMode())
			%client.spectateNextTarget();
		else if(%button == 4 && %camera.isOrbitMode())
			%client.spectatePrevTarget();
		else if(%button == 2)
			%client.spectateChangeMode();
	}

	function serverCmdLight(%client)
	{
		if(%client.isSpectator)
			%client.spectateChangeMode();
	}
};
activatePackage(Slayer_GameConnection);

function GameConnection::spectateNextTarget(%this)
{
	%pos = vectorAdd(%obj.position, "0 0 1.5");
	%camera.setOrbitPointMode(%pos, 6);
}

function GameConnection::spectateAutoCam(%this)
{
	%relStartTransformZ = "0 0 3";
	%relEndTransformZ = "0 0 2";
	%startTransform = vectorAdd(vectorScale(%obj.player.getForwardVector(), -4), %startTransform);
	%endTransform = vectorAdd(vectorScale(%obj.player.getForwardVector(), -1), %endTransform);
	%ray = containerRayCast(%startTransform, %endTransform, %mask, %obj.player);
	%camera.pushBack(%startTransform, 2, "Normal", "Linear");
	%this.setControlCameraFOV(100);
	%this.autoCamRetrySched = %this.scheduleNoQuota(2000, "spectateAutoCam");
}

function Slayer_SpectatePathCamData::onNode(%this, %camera, %node)
{
	if(%node == 2)
		%camera.client.spectateAutoCam();
}

function GameConnection::joinTeam(%this, %flag, %reason, %noRespawn)
{
	%team = %mini.Teams.getTeamFromName(%flag);
	if(isObject(%team))
		%team.addMember(%this, %reason, %noRespawn, %noRespawn);
}

function GameConnection::applyUniform(%this)
{
	switch(%team.uniform)
	{
		case 2:
			hideAllNodes(%player);
			%player.unHideNode(copHat);
			if(!strLen(%player.skinColor))
			{
				%index = getRandom($Slayer::Server::Bots::SkinColorCount);
				%player.skinColor = $Slayer::Server::Bots::SkinColor[%index];
				if(!strLen(%player.skinColor))
					%player.skinColor = "0.9 0.8 0.6 1";
			}
		case 3:
			if(%val $= "TEAMCOLOR")
				%val = %color;
	}
}

function GameConnection::createPlayer(%this, %pos)
{
	%this.player.changeDatablock(%team.playerDatablock);
	for(%i=0; %i < %team.playerDatablock.maxTools; %i++)
		%this.forceEquip(%i, %team.startEquip[%i]);
	if((%ps = %team.playerScale) != 1)
		%this.player.setScale(%ps SPC %ps SPC %ps);
	if(isObject(%team) && %team.respawnTime >= 0)
		%this.setRespawnTime(%team.respawnTime);
}
