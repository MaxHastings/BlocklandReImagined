// Stand-in game mode (CC0): each function the port covers, in the shape its
// patterns read, with this stand-in's own numbers. Not the original's code.
new ScriptGroup(Slayer_GameModeTemplateSG)
{
	className = "Slayer_CTF";
	uiName = "Flag Game";
	useTeams = true;
};

datablock fxDTSBrickData(brickStandInPadData)
{
	brickFile = "./pad.blb";
	uiName = "Stand-in Pad";
};

function createSlayerCTFDatablocks()
{
	datablock fxDtsBrickData(brickSlyrCTFFlagData : brickStandInPadData)
	{
		uiName = "Stand-in Flag Spawn";
		indestructable = 1;
	};
	datablock fxDtsBrickData(brickSlyrCTFFlagReturnData : brickStandInPadData)
	{
		uiName = "Stand-in Return Point";
		indestructable = 1;
	};
	datablock TriggerData(slyrCTF_flagReturnTriggerData)
	{
		tickPeriodMS = 100;
	};
	datablock ShapeBaseImageData(flagImage)
	{
		eyeOffset = "0 0 9";
	};
}

function Slayer_CTF::onMiniGameBrickAdded(%this, %brick, %type)
{
	switch$(%type)
	{
		case "CTF_Flag":
			%brick.resetFlag();
	}
}

function Slayer_CTF::preMinigameReset(%this, %client)
{
	%this.resetFlags(false);
}

function Slayer_CTF::prePlayerDeath(%this, %client, %obj, %killer, %type, %area)
{
	%client.player.dropFlag();
}

function Slayer_CTF::onClientLeaveGame(%this, %client)
{
	%client.player.dropFlag();
}

function Slayer_CTF::onClientLeaveTeam(%this, %team, %client)
{
	%client.player.dropFlag();
}

function Slayer_CTF::onClientDisplayRules(%this, %client)
{
	messageClient(%client, '', "The first team to capture wins.");
}

function Slayer_CTF::isBrickNeutral(%this, %brick)
{
	return %this.minigame.teams.isNeutralColor(%brick.getColorID());
}

function Slayer_CTF::getFlagDisplayName(%this, %brick)
{
	return "Neutral";
}

function Slayer_CTF::isFlagAtHome(%this, %color)
{
	return -1;
}

function Slayer_CTF::onFlagReturn(%this, %client, %team, %brick, %flag)
{
	%points = %this.minigame.CTF_points_Flag;
	%remain = %this.minigame.CTF_flagReturnsToWin - %team.CTF_numFlagReturns - 1;
	%respawnTime = %this.minigame.CTF_flagReturnedRespawnTime;
	%this.minigame.endRound(%team);
}

function Slayer_CTF::onFlagRecovery(%this, %client, %team, %brick, %flag)
{
	%points = %this.minigame.CTF_points_FlagRecovery;
}

function Slayer_CTF::onFlagPickup(%this, %client, %team, %brick, %flag)
{
	%client.player.mountImage(%image, $Slayer::Server::CTF::flagImageSlot);
}

function Slayer_CTF::onFlagDrop(%this, %client, %team, %brick, %flag)
{
	messageAll('', "dropped the flag");
}

function Slayer_CTF::flagRespawnTick(%this, %brick, %item, %ticks)
{
	%this.scheduleNoQuota(1000, "flagRespawnTick", %brick, %item, %ticks ++);
}

function Slayer_CTF::resetFlags(%this, %doNotRespawn)
{
	%br.resetFlag();
}

function FxDtsBrick::resetFlag(%this)
{
	%this.setItem(%item);
}

function Player::dropFlag(%this)
{
	%respawn = %mini.CTF_flagDroppedRespawnTime;
	if(%respawn <= 0)
		%brick.resetFlag();
	%endPos = vectorAdd(%pos,vectorScale(%eyeVect,1.5));
	%raycast = containerRayCast(%pos, %endPos, 0);
	%item.setVelocity(vectorAdd(%item.getVelocity(),vectorScale(%eyeVect,4)));
}

function slyrCTF_FlagItem::onAdd(%this,%obj)
{
	if($Slayer::Server::CTF::flagIdleAnimation[$Slayer::Server::CTF::flagModel] !$= "")
		%obj.playThread(0,$Slayer::Server::CTF::flagIdleAnimation[$Slayer::Server::CTF::flagModel]);
}

function slyrCTF_FlagItem::onPickUp(%this,%flag,%player,%a)
{
	if(getSimTime() - %flag.spawnTime < 250)
		return;
	if(%color == %team.color)
	{
		if(%mini.CTF_flagRecovery == 1) {}
		else if(%mini.CTF_flagRecovery == 2) {}
	}
	if(%neutral && %mini.CTF_neutralFlags == 0) {}
	else if(!%neutral && %mini.CTF_neutralFlags == 2) {}
	if(%mini.CTF_requireEnemyPlayers) {}
}

function slyrCTF_flagReturnTriggerData::onEnterTrigger(%this,%trigger,%player)
{
	if(%color == %team.color || (%neutral && %slyrType $= "CTF_FlagReturn"))
	{
		if(%mini.CTF_flagReturnOnlyAtReturnBrick && %slyrType !$= "CTF_FlagReturn")
			return;
		if(!%mini.CTF_returnWithoutOwn)
		{
			if(!%mini.gameMode.isFlagAtHome(%team.color))
				return;
		}
	}
}

function serverCmdDropFlag(%client)
{
	if(!%client.minigame.CTF_manualFlagDrop)
		return;
	%client.player.dropFlag();
}
