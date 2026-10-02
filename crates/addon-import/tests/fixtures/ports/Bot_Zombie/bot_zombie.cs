datablock fxDTSBrickData (BrickZombie_HoleSpawnData)
{
	brickFile = "./hole.blb";
	category = "Special";
	subCategory = "Holes";
	uiName = "Zombie Hole";
	isBotHole = 1;
	holeBot = "ZombieHoleBot";
};

datablock PlayerData(ZombieHoleBot : PlayerStandardArmor)
{
	uiName = "";
	canJet = 0;
	maxForwardSpeed = 6;
	maxdamage = 40;
	isHoleBot = 1;
	hType = shamblers;
	hName = "Shambler";
	hWander = 1;
	hSpawnDist = 24;
	hSearch = 1;
	hSearchRadius = 45;
	hMelee = 1;
	hAttackDamage = 9;
};

function ZombieHoleBot::onAdd(%this,%obj)
{
	armor::onAdd(%this,%obj);
	%obj.playthread(1,"ArmReadyBoth");
	%obj.headColor = "0.5 0.6 0.4 1";
	%obj.lhandColor = "0.5 0.6 0.4 1";
	%obj.rhandColor = "0.5 0.6 0.4 1";
	%obj.chestColor = "0.3 0.3 0.3 1";
	%obj.larmColor = "0.4 0.1 0.1 1";
	%obj.rarmColor = "0.4 0.1 0.1 1";
	%obj.hipColor = "0.1 0.1 0.3 1";
	%obj.llegColor = "0.1 0.1 0.3 1";
	%obj.rlegColor = "0.1 0.1 0.3 1";
	%obj.faceName = "smileyEvil1";
	GameConnection::ApplyBodyParts(%obj);
	GameConnection::ApplyBodyColors(%obj);
}

function ZombieHoleBot::onBotLoop(%this,%obj)
{
}

function ZombieHoleBot::onBotFollow(%this,%obj,%targ)
{
}

function ZombieHoleBot::onBotCollision(%this,%obj,%col,%normal,%speed)
{
}

function holeZombieInfect(%obj,%col)
{
	%col.setDataBlock(ZombieHoleBot);
	%col.setHealth(%col.getDataBlock().maxdamage);
	%col.hType = %obj.hType;
	%col.hIsInfected = 1;
}

package holeZombiePackage
{
	function armor::onCollision(%this,%obj,%col,%a,%b,%c,%d)
	{
		if(%obj.hIsInfected && %col.isBot && checkHoleBotTeams(%obj,%col))
		{
			if(%col.getDamagePercent() >= 0.5)
				holeZombieInfect(%obj,%col);
		}
		parent::onCollision(%this,%obj,%col,%a,%b,%c,%d);
	}
};
activatePackage(holeZombiePackage);
