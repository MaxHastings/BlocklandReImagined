datablock fxDTSBrickData (BrickSharkBot_HoleSpawnData)
{
	brickFile = "./hole.blb";
	category = "Special";
	subCategory = "Holes";
	uiName = "Shark Hole";
	isBotHole = 1;
	holeBot = "SharkHoleBot";
};

datablock PlayerData(SharkHoleBot : PlayerStandardArmor)
{
	uiName = "";
	canJet = 0;
	maxForwardSpeed = 0;
	maxUnderwaterForwardSpeed = 0;
	maxdamage = 200;
	isHoleBot = 1;
	hType = Biters;
	hName = "Biter";
	hWander = 1;
	hSpawnDist = 30;
	hSearch = 1;
	hSearchRadius = 85;
	hMelee = 1;
	hAttackDamage = 0;
};

// The original package declares buoyancy variants of the same body. These
// stand-ins exercise conversion and porting of inherited helper archetypes.
datablock PlayerData(SharkHoleBotTop : SharkHoleBot)
{
	density = 0.1;
};
datablock PlayerData(SharkHoleBotBottom : SharkHoleBot)
{
	density = 10;
};

function SharkHoleBot::onAdd(%this,%obj)
{
	armor::onAdd(%this,%obj);
	%color[%a++] = "0.8 0.8 0.85 1";
	%color[%a++] = "0.3 0.3 0.3 1";
	%obj.setNodeColor("ALL",%color[getRandom(1,%a)]);
	%obj.hideNode(helmet);
	%obj.hideNode(visor);
	%obj.setNodeColor("lArm","0.95 0.95 0.95 1");
	%obj.setNodeColor("rArm","0.95 0.95 0.95 1");
}

function SharkHoleBot::onEnterLiquid(%data,%obj,%coverage,%type)
{
	%obj.hFishOutOfWater = 0;
}

function SharkHoleBot::onBotLoop(%this,%obj)
{
	if(%obj.getWaterCoverage() && !%obj.isSwimming)
	{
		%obj.playThread(0,swim);
		%obj.isSwimming = 1;
	}
	if(!%obj.getWaterCoverage() && isObject(getMiniGameFromObject(%obj)))
		%obj.hFishOutOfWater++;
	else
		%obj.hFishOutOfWater = 0;
	if(%obj.hFishOutOfWater >= 3)
		%obj.kill();
}

function SharkHoleBot::onBotCollision(%this,%obj,%col,%normal,%speed)
{
	if(checkHoleBotTeams(%obj,%col))
	{
		%obj.hAttackDamage = 35;
		%obj.hMeleeAttack(%col);
		%obj.hAttackDamage = 0;
	}
}
