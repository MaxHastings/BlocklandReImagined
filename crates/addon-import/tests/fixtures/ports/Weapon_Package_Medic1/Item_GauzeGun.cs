datablock AudioProfile(medigunShot1Sound)
{
   filename = "./mend.wav";
   description = AudioClose3d;
   preload = true;
};
datablock AudioProfile(medigunReloadSound)
{
   filename = "./mend.wav";
   description = AudioClose3d;
   preload = true;
};

datablock ProjectileData(medigunProjectile)
{
   projectileShapeName = "./dart.dts";
   directDamage = 0;
   impactImpulse = 0;
   verticalImpulse = 0;
   explosion = gunExplosion;
   muzzleVelocity = 120;
   velInheritFactor = 0;
   armingDelay = 0;
   lifetime = 3000;
   fadeDelay = 2500;
   isBallistic = false;
   gravityMod = 0.0;
};

datablock ItemData(medigunItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./dartgun.dts";
   uiName = "Stand-in Dart Mender";
   image = medigunImage;
   canDrop = true;
};

datablock ShapeBaseImageData(medigunImage)
{
   shapeFile = "./dartgun.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = medigunItem;
   projectile = medigunProjectile;
   projectileType = Projectile;
   armReady = true;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.1;
   stateTransitionOnTimeout[0] = "Ready";

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "Fire";
   stateAllowImageChange[1] = true;

   stateName[2] = "Fire";
   stateTransitionOnTimeout[2] = "Ready";
   stateTimeoutValue[2] = 0.3;
   stateFire[2] = true;
   stateAllowImageChange[2] = false;
   stateScript[2] = "onFire";
   stateWaitForTimeout[2] = true;
};

datablock ParticleData(healParticle)
{
   dragCoefficient = 2.0;
   lifetimeMS = 900;
   textureName = "base/data/particles/dot";
   colors[0] = "0.4 1 0.4 0.2";
   colors[1] = "0.4 1 0.4 0";
   sizes[0] = 0.4;
   sizes[1] = 0.1;
   times[0] = 0.0;
   times[1] = 1.0;
};

datablock ParticleEmitterData(healEmitter)
{
   ejectionPeriodMS = 10;
   ejectionVelocity = 2.0;
   velocityVariance = 1.0;
   thetaMin = 0;
   thetaMax = 180;
   phiVariance = 360;
   particles = "healParticle";
};

// Mends its wearer a little each pass.
datablock ShapeBaseImageData(medigunHealImage)
{
   shapeFile = "base/data/shapes/empty.dts";
   mountPoint = 6;
   offset = "0 0 -1";

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.05;
   stateTransitionOnTimeout[0] = "Heal1";
   stateScript[0] = "onActivate";

   stateName[1] = "Heal1";
   stateTransitionOnTimeout[1] = "Done";
   stateTimeoutValue[1] = 0.05;
   stateScript[1] = "onHeal";
   stateEmitter[1] = healEmitter;
   stateEmitterTime[1] = 0.05;
   stateEmitterNode[1] = "muzzleNode";

   stateName[2] = "Done";
   stateTransitionOnTimeout[2] = "Activate";
   stateTimeoutValue[2] = 0.05;
};

function medigunHealImage::onMount(%this, %obj, %slot)
{
	%obj.healing = 0;
}

function medigunHealImage::onHeal(%this, %obj, %slot)
{
	if(%obj.getDamagePercent() < 1.0)
	{
	%obj.setDamageLevel(%obj.getDamageLevel() - 3);
	%obj.healing++;
	if(%obj.healing >= 6)
	{
	%obj.unMountImage(%slot);
	}
	}
	else
	{
	%obj.unMountImage(%slot);
	}
}

function medigunImage::onFire(%this, %obj, %slot)
{
	%obj.playThread(2, shiftAway);
	serverPlay3D(medigunShot1Sound, %obj.getPosition());
	return Parent::onFire(%this, %obj, %slot);
}

function medigunProjectile::onCollision(%this, %obj, %col, %pos, %fade)
{
	TT_projectileHeal(%obj, %col, 12);
	Parent::onCollision(%this, %obj, %col, %pos, %fade);
}
