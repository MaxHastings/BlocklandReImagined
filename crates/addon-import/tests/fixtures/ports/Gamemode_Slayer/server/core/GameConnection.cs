// Stand-in (CC0): the death and score shapes the port reads. Not the
// original's code.
package Slayer_GameConnection
{
	function GameConnection::onDeath(%this, %obj, %killer, %type, %area)
	{
		if(%this.getLives() > 0)
		{
			%this.addLives(-1);
			if(%this.getLives() <= 0)
				%this.setDead(1);
		}
		%this.centerPrint("\c5You have \c30 \c5lives left.", 3);
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
