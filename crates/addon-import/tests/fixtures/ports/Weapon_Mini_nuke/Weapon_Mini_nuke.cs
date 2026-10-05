// Stand-in for the Weapon_Mini_nuke port tests (CC0): a shoulder launcher
// held up with both arms whose missile ends in a very large blast. It runs a
// script of the base game's rocket launcher and names a particle texture the
// game does not have, as the community Add-On does.
exec("add-ons/weapon_rocket_launcher/weapon_rocket launcher.cs");

datablock ParticleData(standinNukeSparkParticle)
{
   textureName          = "base/data/particles/star";
   lifetimeMS           = 1200;
   lifetimeVarianceMS   = 50;
   gravityCoefficient   = 3;
   colors[0]            = "1 0.6 0.2 0.5";
   colors[1]            = "1 1 1 0";
   sizes[0]             = 40;
   sizes[1]             = 1;
   useInvAlpha          = true;
};

datablock ParticleEmitterData(standinNukeSparkEmitter)
{
   ejectionPeriodMS = 4;
   periodVarianceMS = 0;
   ejectionVelocity = 60;
   ejectionOffset   = 5;
   thetaMin         = 0;
   thetaMax         = 180;
   phiVariance      = 360;
   particles = "standinNukeSparkParticle";
};

datablock ExplosionData(standinNukeExplosion)
{
   lifeTimeMS = 300;
   emitter[0] = standinNukeSparkEmitter;

   shakeCamera = true;
   camShakeFreq = "8.0 9.0 8.0";
   camShakeAmp = "2.0 6.0 2.0";
   camShakeDuration = 2.0;
   camShakeRadius = 15.0;

   damageRadius = 20;
   radiusDamage = 5000;
   impulseRadius = 30;
   impulseForce = 60000;
};

datablock ProjectileData(standinNukeProjectile)
{
   directDamage        = 120;
   directDamageType    = $DamageType::RocketDirect;
   radiusDamageType    = $DamageType::RocketRadius;
   explosion           = standinNukeExplosion;

   brickExplosionRadius = 20;
   brickExplosionImpact = false;
   brickExplosionForce  = 50;
   brickExplosionMaxVolume = 200;
   brickExplosionMaxVolumeFloating = 300;

   muzzleVelocity      = 55;
   velInheritFactor    = 1.0;
   lifetime            = 30000;
   fadeDelay           = 3000;
   isBallistic         = true;
   gravityMod          = 1;
};

datablock ItemData(mininukeLauncherItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./launcher.dts";
   uiName = "Stand-in Mini-Nuke";
   image = mininukeLauncherImage;
   canDrop = true;
};

datablock ShapeBaseImageData(mininukeLauncherImage)
{
   shapeFile = "./launcher.dts";
   mountPoint = 0;
   correctMuzzleVector = true;
   className = "WeaponImage";
   item = mininukeLauncherItem;
   projectile = standinNukeProjectile;
   projectileType = Projectile;
   armReady = true;
   minShotTime = 1500;

   stateName[0]                    = "Activate";
   stateTimeoutValue[0]            = 0.2;
   stateTransitionOnTimeout[0]     = "Ready";

   stateName[1]                    = "Ready";
   stateTransitionOnTriggerDown[1] = "Fire";
   stateAllowImageChange[1]        = true;

   stateName[2]                    = "Fire";
   stateTransitionOnTimeout[2]     = "Reload";
   stateTimeoutValue[2]            = 0.1;
   stateFire[2]                    = true;
   stateAllowImageChange[2]        = false;
   stateScript[2]                  = "onFire";
   stateWaitForTimeout[2]          = true;

   stateName[3]                    = "Reload";
   stateTransitionOnTriggerUp[3]   = "Ready";
};

function mininukeLauncherImage::onMount(%this, %obj, %slot)
{
   Parent::onMount(%this, %obj, %slot);
   %obj.playThread(0, armReadyBoth);
}

function mininukeLauncherImage::onUnMount(%this, %obj, %slot)
{
   Parent::onMount(%this, %obj, %slot);
   %obj.playThread(0, root);
}
