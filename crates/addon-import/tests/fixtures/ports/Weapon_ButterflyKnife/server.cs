// Stand-in for the Weapon_ButterflyKnife port tests (CC0): a charged stab
// through the image's projectile and a quick jab through a second one, with
// the community knife's datablock and function names and its own numbers.
datablock ProjectileData(butterflyknifeProjectile)
{
   directDamage        = 20;
   muzzleVelocity      = 40;
   velInheritFactor    = 1;
   lifetime            = 120;
   fadeDelay           = 80;
   isBallistic         = false;
   gravityMod          = 0.0;
};

datablock ProjectileData(butterflyknifekillProjectile)
{
   directDamage        = 80;
   muzzleVelocity      = 40;
   velInheritFactor    = 1;
   lifetime            = 120;
   fadeDelay           = 80;
   isBallistic         = false;
   gravityMod          = 0.0;
};

datablock ItemData(butterflyknifeItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./knife.dts";
   uiName = "Stand-in Knife";
   image = butterflyknifeImage;
   canDrop = true;
};

datablock ShapeBaseImageData(butterflyknifeImage)
{
   shapeFile = "./knife.dts";
   mountPoint = 0;
   correctMuzzleVector = true;
   className = "WeaponImage";
   item = butterflyknifeItem;
   projectile = butterflyknifekillProjectile;
   projectileType = Projectile;
   armReady = true;

   stateName[0]                    = "Activate";
   stateTimeoutValue[0]            = 0.4;
   stateTransitionOnTimeout[0]     = "Ready";

   stateName[1]                    = "Ready";
   stateTransitionOnTriggerDown[1] = "Charge";
   stateAllowImageChange[1]        = true;

   stateName[2]                    = "Charge";
   stateTransitionOnTimeout[2]     = "Armed";
   stateTimeoutValue[2]            = 0.5;
   stateScript[2]                  = "onCharge";
   stateWaitForTimeout[2]          = false;
   stateTransitionOnTriggerUp[2]   = "Jab";
   stateAllowImageChange[2]        = false;

   stateName[3]                    = "Jab";
   stateTransitionOnTimeout[3]     = "StopFire";
   stateTimeoutValue[3]            = 0.3;
   stateFire[3]                    = true;
   stateScript[3]                  = "onFiretwo";
   stateWaitForTimeout[3]          = true;

   stateName[4]                    = "StopFire";
   stateTransitionOnTimeout[4]     = "Ready";
   stateTimeoutValue[4]            = 0.3;
   stateWaitForTimeout[4]          = true;
   stateScript[4]                  = "onStopFire";

   stateName[5]                    = "Armed";
   stateTransitionOnTriggerUp[5]   = "Stab";
   stateAllowImageChange[5]        = false;

   stateName[6]                    = "Stab";
   stateTransitionOnTimeout[6]     = "Ready";
   stateTimeoutValue[6]            = 0.3;
   stateFire[6]                    = true;
   stateScript[6]                  = "onFire";
   stateWaitForTimeout[6]          = true;
};

function butterflyknifeImage::onCharge(%this, %obj, %slot)
{
   %obj.playThread(2, spearReady);
}

// Defined twice, as in the community knife: Torque keeps the later one.
function butterflyknifeImage::onfiretwo(%this, %obj, %slot)
{
   %obj.playThread(2, armAttack);
   return;
}

function butterflyknifeImage::onStopFire(%this, %obj, %slot)
{
   %obj.playThread(2, root);
}

function butterflyknifeImage::onFire(%this, %obj, %slot)
{
   %obj.playThread(2, spearThrow);
   Parent::onFire(%this, %obj, %slot);
}

function butterflyknifeImage::onFiretwo(%this, %obj, %slot)
{
   %projectile = butterflyknifeProjectile;
   %spread = 0.00001;
   %shellcount = 1;

   for(%shell = 0; %shell < %shellcount; %shell++)
   {
      %vector = %obj.getMuzzleVector(%slot);
      %objectVelocity = %obj.getVelocity();
      %vector1 = VectorScale(%vector, %projectile.muzzleVelocity);
      %vector2 = VectorScale(%objectVelocity, %projectile.velInheritFactor);
      %velocity = VectorAdd(%vector1, %vector2);
      %x = (getRandom() - 0.5) * 2 * 3.1415926 * %spread;
      %y = (getRandom() - 0.5) * 2 * 3.1415926 * %spread;
      %z = (getRandom() - 0.5) * 2 * 3.1415926 * %spread;
      %mat = MatrixCreateFromEuler(%x @ " " @ %y @ " " @ %z);
      %velocity = MatrixMulVector(%mat, %velocity);

      %p = new (%this.projectileType)()
      {
         dataBlock = %projectile;
         initialVelocity = %velocity;
         initialPosition = %obj.getMuzzlePoint(%slot);
         sourceObject = %obj;
         sourceSlot = %slot;
         client = %obj.client;
      };
      MissionCleanup.add(%p);
   }
   return %p;
}
