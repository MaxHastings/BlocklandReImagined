// The concussion grenade: thrown, it goes off on its second knock.
datablock AudioProfile(standinTossSound)
{
   filename = "./toss.wav";
   description = AudioClosest3d;
   preload = true;
};
datablock AudioProfile(standinKnockSound : standinTossSound) { filename = "./knock.wav"; };

datablock ExplosionData(standinConcExplosion)
{
   lifeTimeMS = 100;
   soundProfile = standinBoomSound;
   damageRadius = 4;
   radiusDamage = 60;
};

AddDamageType("StandinConc", '%1 blew up', '%2 blew up %1', 0.5, 0);

datablock ProjectileData(tierfragGrenadeProjectile)
{
   projectileShapeName = "./conc.dts";
   directDamage = 5;
   directDamageType = $DamageType::StandinConc;
   radiusDamageType = $DamageType::StandinConc;
   explosion = standinConcExplosion;
   muzzleVelocity = 30;
   velInheritFactor = 0;
   armingDelay = 2000;
   lifetime = 2500;
   fadeDelay = 2400;
   bounceElasticity = 0.5;
   bounceFriction = 0.2;
   isBallistic = true;
   explodeOnDeath = true;
};

datablock ItemData(tierfragGrenadeItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./conc.dts";
   uiName = "Stand-in Conc";
   image = tierfragGrenadeImage;
   canDrop = true;
   TT_ammoType = "fragNades";
   TT_grenade = true;
};

datablock ShapeBaseImageData(tierfragGrenadeImage)
{
   shapeFile = "./conc.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = tierfragGrenadeItem;
   projectile = tierfragGrenadeProjectile;
   projectileType = Projectile;
   armReady = true;

   stateName[0]                    = "Activate";
   stateTimeoutValue[0]            = 0.02;
   stateTransitionOnTimeout[0]     = "Ready";

   stateName[1]                    = "Ready";
   stateTransitionOnTriggerDown[1] = "Tick";
   stateScript[1]                  = "onReady";

   stateName[2]                    = "Tick";
   stateTimeoutValue[2]            = 0.1;
   stateTransitionOnTimeout[2]     = "Charge";
   stateAllowImageChange[2]        = false;
   stateSound[2]                   = standinMoveSound;
   stateScript[2]                  = "onTick";

   stateName[3]                    = "Charge";
   stateTimeoutValue[3]            = 0.1;
   stateTransitionOnTimeout[3]     = "Armed";
   stateScript[3]                  = "onCharge";

   stateName[4]                    = "Armed";
   stateTimeoutValue[4]            = 0.1;
   stateTransitionOnTimeout[4]     = "Fire";
   stateScript[4]                  = "onArmed";

   stateName[5]                    = "Fire";
   stateTimeoutValue[5]            = 0.4;
   stateTransitionOnTimeout[5]     = "Activate";
   stateFire[5]                    = true;
   stateScript[5]                  = "onFire";
   stateAllowImageChange[5]        = false;
};

function tierfragGrenadeImage::onArmed(%this, %obj, %slot)
{
   %this.TT_displayAmmo(%obj, 1);
   %obj.playThread(2, shiftAway);
}

function tierfragGrenadeImage::onFire(%this, %obj, %slot)
{
   if(%this.TT_canFire(%obj))
   {
      %p = Parent::onFire(%this, %obj, %slot);
      %this.TT_decrementAmmo(%obj);
      %obj.playThread(2, shiftTo);
      serverPlay3D(standinTossSound, %obj.getPosition());
   }
   if(%this.TT_needsAmmo(%obj))
   {
      %obj.unMountImage(0);
   }
   %this.TT_displayAmmo(%obj, 1);
   return %p;
}

function tierfragGrenadeImage::onReady(%this, %obj, %slot)
{
   %this.TT_displayAmmo(%obj, 0);
   if(%this.TT_needsAmmo(%obj))
   {
      %obj.unMountImage(0);
      %obj.playThread(1, root);
   }
}

function tierfragGrenadeImage::onTick(%this, %obj, %slot)
{
   if(%this.TT_needsAmmo(%obj))
   {
      %obj.unMountImage(0);
      %obj.playThread(1, root);
   }
   else
      %obj.playThread(2, shiftLeft);
}

function tierfragGrenadeImage::onCharge(%this, %obj, %slot)
{
   if(%this.TT_needsAmmo(%obj))
   {
      %obj.unMountImage(0);
      %obj.playThread(1, root);
   }
}

function tierfragGrenadeProjectile::onCollision(%this, %obj, %col, %fade, %pos, %normal)
{
   serverPlay3D(standinKnockSound, %obj.getTransform());
   if(%obj.explodeTicks >= 0) { %obj.explodeTicks += 1; }
   if(%obj.explodeTicks == 2) { %obj.explode(); }
}
