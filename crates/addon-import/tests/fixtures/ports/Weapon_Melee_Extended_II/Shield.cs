// Stand-in riot shield, written for these tests: our own numbers, laid
// out as Tier+Tactical's shield is so its port applies.
exec("./Support_SpecialKills.cs");
addSpecialDamageMsg("Reflected", "%2 [sent back]%3%1", "[sent back] %3%1");

function isSpecialKill_Reflected(%this, %sourceObject, %sourceClient, %mini)
{
	return (%sourceObject.reflectTime > 0);
}

datablock AudioProfile(ShieldBreakSound)
{
   filename = "./snap.wav";
   description = AudioClose3d;
   preload = true;
};
datablock AudioProfile(ShieldHit1Sound : ShieldBreakSound) { filename = "./twang.wav"; };
datablock AudioProfile(ShieldHit2Sound : ShieldBreakSound) { filename = "./bing.wav"; };

datablock ExplosionData(shieldRiotTTExplosion)
{
   lifeTimeMS = 150;
   soundProfile = ShieldBreakSound;
};

AddDamageType("ShieldBash", '%1 bashed themselves', '%2 [bash] %1', 1, 1);
datablock ProjectileData(shieldRiotTTBashProjectile)
{
   directDamage = 60;
   directDamageType = $DamageType::ShieldBash;
   impactImpulse = 1000;
   verticalImpulse = 100;
   muzzleVelocity = 120;
   velInheritFactor = 0;
   armingDelay = 0;
   lifetime = 66;
   fadeDelay = 70;
   isBallistic = false;
   gravityMod = 0.0;
};

datablock ProjectileData(shieldRiotTTProjectile)
{
   directDamage = 0;
   explosion = shieldRiotTTExplosion;
   muzzleVelocity = 100;
   lifetime = 66;
   isBallistic = false;
};

datablock ItemData(RiotTTShieldItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Barrier";
   image = shieldRiotTTImage;
   canDrop = true;
};

datablock ShapeBaseImageData(ShieldRiotTTImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = RiotTTShieldItem;
   projectile = shieldRiotTTBashProjectile;
   projectileType = "Projectile";
   armReady = true;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.2;
   stateScript[0] = "onActivate";
   stateTransitionOnTimeout[0] = "Ready";

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";
   stateAllowImageChange[1] = true;

   stateName[2] = "PreFire";
   stateScript[2] = "onPreFire";
   stateAllowImageChange[2] = false;
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "Fire";

   stateName[3] = "Fire";
   stateTransitionOnTimeout[3] = "CheckFire";
   stateTimeoutValue[3] = 0.4;
   stateFire[3] = true;
   stateAllowImageChange[3] = false;
   stateScript[3] = "onFire";
   stateWaitForTimeout[3] = true;

   stateName[4] = "CheckFire";
   stateTransitionOnTriggerDown[4] = "PreFire";
   stateTransitionOnTriggerUp[4] = "Ready";
};

function ShieldRiotTTImage::onActivate(%this, %obj, %slot)
{
	%obj.playthread(1, armReadyBoth);
}

function shieldRiotTTImage::onUnMount(%this, %obj, %slot)
{
	fixArmReady(%obj);
}

function ShieldRiotTTImage::onPrefire(%this, %obj, %slot)
{
	%obj.playthread(2, shiftUp);
}

function ShieldRiotTTImage::onFire(%this, %obj, %slot)
{
	Parent::onFire(%this, %obj, %slot);
	%obj.playthread(2, shiftDown);
}

package StandinShield
{
   function ProjectileData::damage(%this, %obj, %col, %fade, %pos, %normal)
   {
	%aimVec = %fX * %eXY SPC %fY * %eXY SPC %eZ;
	if(%image0 == shieldRiotTTImage.getID() && %state0 $= "Ready")
	{
	if(%eZ > 0.7)
	%shielded = (getword(%pos, 2) > getword(%col.getWorldBoxCenter(), 2) - 3 * %scale);
	else if(%ez < -0.8)
	%shielded = (getword(%pos, 2) < getword(%col.getWorldBoxCenter(), 2) - 4 * %scale);
	else
	%shielded = (vectorDot(vectorNormalize(%obj.getVelocity()), %aimVec) < 0);
	%damageScale = 0.1;
	%reflect = 1;
	%reflectPoint = vectorAdd(%col.getHackPosition(), vectorScale(%reflectVector, vectorLen(%col.getVelocity()) / 5 + 1));
	%impulseScale = 0.5;
	if(%shielded)
	%col.spawnExplosion(hammerProjectile, getWord(%col.getScale(), 2));
	if(getSimTime() - %obj.reflectTime < 500)
	%reflect = 0;
	%col.shieldHP = $Pref::Server::TT::ShieldDurability;
	%col.shieldHP--;
	if(%col.shieldHP <= 0)
	{
	if(%col.currtool == %i)
	{
	%col.unmountImage(0);
	%col.spawnExplosion(shieldRiotTTProjectile, getWord(%col.getScale(), 2));
	%sound = getRandom(1, 3);
	switch(%sound)
	{
	case 1:
	serverPlay3D(ShieldHit2Sound, %pos);
	case 2:
	serverPlay3D(ShieldHit1Sound, %pos);
	case 3:
	serverPlay3D(ShieldHit1Sound, %pos);
	default:
	error("no sound");
	%vec = vectorScale(%reflectVector, vectorLen(%obj.getVelocity()));
	%vel = vectorAdd(%vec, vectorScale(%col.getVelocity(), %obj.dataBlock.velInheritFactor));
	sourceObject = %col;
	}}}}
}

   function ProjectileData::radiusDamage(%this, %obj, %col, %distanceFactor, %pos, %damageAmt)
   {
	if(%obj.damageCancel[%col])
	return;
}

   function ProjectileData::radiusImpulse(%this, %obj, %col, %a, %pos, %b, %c)
   {
	if(%obj.damageCancel[%col])
	%b *= %obj.impulseScale[%col];
}

   function ProjectileData::impactImpulse(%this, %obj, %col, %a)
   {
	%this.impactImpulse *= %obj.impulseScale[%col];
}

   function ShapeBase::damage(%this, %sourceObject, %pos, %directDamage, %damageType)
   {
	if(%shielded && $Pref::Server::TT::ShieldCancelFalling)
	%directDamage = %directDamage / 8;
	%shielded = (vectorDot(%attackvec, %aimVec) < 0);
	%damageScale = 0.25;
	if(%directDamage > 5000 || %pos $= "" || vectorDist(%pos, %this.getPosition()) < 0.1)
	%shielded = 0;
	if(%shielded)
	%directDamage *= %damageScale;
}
};
activatePackage(StandinShield);
