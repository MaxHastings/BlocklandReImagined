// Stand-in (CC0): a module that wraps a core function in a package, as the
// original's fly-through camera does, with the shapes the port reads and
// this stand-in's numbers. Ports read the core definition.
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	title = "Play At Beginning of Round";
	defaultValue = true;
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	title = "Countdown During Fly-Thru";
	defaultValue = false;
};
new ScriptObject(Slayer_PrefSO : Slayer_DefaultPrefSO)
{
	title = "Rounds Between Fly-Thrus";
	defaultValue = false;
};

datablock PathCameraData(Slayer_PathCamData : Observer)
{
	maxNodes = 6;
	defaultSpeed = 9;
};

function serverCmdSetJump(%client, %speed, %type, %path)
{
	if(%type $= "") %type = "Kink";
}

function Slayer_MinigameSO::createPathCamera(%this, %datablock, %transform, %speed, %type, %path)
{
	%datablock.addNode(%this.flyCam, %transform, %speed, %type, %path, 0);
	%this.flyCam.setPosition(1.0);
	%this.flyCam.popFront();
}

function Slayer_PathCamData::onNode(%this, %cam, %node)
{
	if(%node == %cam.numNodes - 1)
	{
		if(%cam.startRound && !%mini.flyCam_playDuringCountdown)
			%mini.startRound();
	}
	else if(%cam.nodeJump[%node + 1])
		%cam.setPosition(%node + 1);
}

package Slayer_Stand_In_Module
{
	function Slayer_MinigameSO::preRoundCountdownTick(%this, %ticks)
	{
		%parent = parent::preRoundCountdownTick(%this, %ticks);
		%remain = %this.preRoundSeconds - %ticks;
		if(%remain <= 0)
			%cl.flyCam_lastControlObject = "";
		return %parent;
	}

	function Slayer_MinigameSO::onReset(%this)
	{
		if(%this.flyCam_roundsPassed > %this.flyCam_roundsBetweenFlyThroughs)
			%this.startFlyThrough(1);
		return parent::onReset(%this);
	}
};
activatePackage(Slayer_Stand_In_Module);

registerOutputEvent("Minigame", "StartFlyThrough", "", 0);
$Slayer::Server::Events::RestrictedEvent__["Minigame", "startFlyThrough"] = 3;

function serverCmdDeleteFlyCam(%client)
{
	%mini = getMinigameFromObject(%client);
	%mini.flyCam.delete();
	%file = %mini.configFile @ ".pathcam";
	fileDelete(%file);
}

package Slayer_Stand_In_Path_Files
{
	function Slayer_PrefHandlerSG::importMinigamePreferences(%this, %path, %mini)
	{
		%file = %path @ ".pathcam";
		%cam = %mini.createPathCamera(%file);
		Slayer_PathCamData.addNode(%cam);
		return parent::importMinigamePreferences(%this, %path, %mini);
	}

	function Slayer_PrefHandlerSG::exportMinigamePreferences(%this, %path, %mini)
	{
		%file = %path @ ".pathcam";
		%line = %mini.flyCam.nodeJump[0];
		return parent::exportMinigamePreferences(%this, %path, %mini);
	}
};
activatePackage(Slayer_Stand_In_Path_Files);
