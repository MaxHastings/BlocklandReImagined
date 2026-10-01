// The stick grenade: held up while the trigger is down, thrown on release.
AddDamageType("StandinStick", '%1 blew up', '%2 blew up %1', 0.5, 0);

datablock ProjectileData(tierstickGrenadeProjectile)
{
   projectileShapeName = "./stick.dts";
   directDamage = 5;
   directDamageType = $DamageType::StandinStick;
   radiusDamageType = $DamageType::StandinStick;
   explosion = standinConcExplosion;
   muzzleVelocity = 25;
   velInheritFactor = 0;
   armingDelay = 2900;
   lifetime = 3000;
   fadeDelay = 2900;
   bounceElasticity = 0.1;
   isBallistic = true;
   explodeOnDeath = true;
};

datablock ItemData(tierstickGrenadeItem : tierfragGrenadeItem)
{
   shapeFile = "./stick.dts";
   uiName = "Stand-in Stick";
   image = tierstickGrenadeImage;
   TT_ammoType = "stickNades";
};

datablock ShapeBaseImageData(tierstickGrenadeImage)
{
   shapeFile = "./stick.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = tierstickGrenadeItem;
   projectile = tierstickGrenadeProjectile;
   projectileType = Projectile;
   armReady = true;

   stateName[0]                    = "Activate";
   stateTimeoutValue[0]            = 0.02;
   stateTransitionOnTimeout[0]     = "Ready";

   stateName[1]                    = "Ready";
   stateTransitionOnTriggerDown[1] = "Pull";
   stateScript[1]                  = "onReady";

   stateName[2]                    = "Pull";
   stateTimeoutValue[2]            = 0.1;
   stateTransitionOnTimeout[2]     = "Charge";
   stateScript[2]                  = "onPull";

   stateName[3]                    = "Charge";
   stateTimeoutValue[3]            = 0.1;
   stateTransitionOnTimeout[3]     = "Armed";
   stateScript[3]                  = "onCharge";

   stateName[4]                    = "Armed";
   stateTransitionOnTriggerUp[4]   = "Fire";
   stateScript[4]                  = "onArmed";

   stateName[5]                    = "Fire";
   stateTimeoutValue[5]            = 0.4;
   stateTransitionOnTimeout[5]     = "Activate";
   stateFire[5]                    = true;
   stateScript[5]                  = "onFire";
   stateAllowImageChange[5]        = false;
};

function tierstickGrenadeImage::onArmed(%this, %obj, %slot)
{
   %obj.playThread(2, spearReady);
   %obj.setImageAmmo(0, 0);
}

function tierstickGrenadeImage::onFire(%this, %obj, %slot)
{
   if(!%this.TT_needsAmmo(%obj))
   {
      %p = Parent::onFire(%this, %obj, %slot);
      %this.TT_decrementAmmo(%obj);
      %obj.playThread(2, spearThrow);
      serverPlay3D(standinTossSound, %obj.getPosition());
   }
   if(%this.TT_needsAmmo(%obj))
   {
      %obj.unMountImage(0);
   }
   %obj.setImageAmmo(0, 1);
   return %p;
}

function tierstickGrenadeImage::onReady(%this, %obj, %slot)
{
   if(%this.TT_needsAmmo(%obj))
   {
      %obj.unMountImage(0);
   }
}

function tierstickGrenadeImage::onPull(%this, %obj, %slot)
{
   if(%this.TT_needsAmmo(%obj))
   {
      %obj.unMountImage(0);
   }
   else
      %obj.playThread(2, shiftRight);
}

function tierstickGrenadeImage::onCharge(%this, %obj, %slot)
{
   if(%this.TT_needsAmmo(%obj))
   {
      %obj.unMountImage(0);
   }
}
