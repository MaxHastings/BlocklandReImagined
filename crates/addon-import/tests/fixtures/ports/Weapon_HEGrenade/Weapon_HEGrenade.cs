// Stand-in for the Weapon_HEGrenade port tests (CC0): a thrown grenade with
// a fuse that is used up when thrown, with the community HE Grenade's
// datablock and function names and its own numbers.
datablock AudioProfile(hegrenadeBounceSound)
{
   filename    = "./bounce.wav";
   description = AudioClosest3d;
   preload = true;
};

datablock DebrisData(hegrenadePinDebris)
{
   shapeFile = "./pin.dts";
   lifetime = 3.0;
   elasticity = 0.4;
   friction = 0.3;
   numBounces = 2;
   fade = true;
   gravModifier = 1.5;
};

datablock ExplosionData(hegrenadeExplosion)
{
   lifeTimeMS = 200;
   impulseRadius = 12;
   impulseForce = 2000;
   damageRadius = 9;
   radiusDamage = 120;
};

datablock ProjectileData(hegrenadeProjectile)
{
   projectileShapeName = "./thrown.dts";
   directDamage        = 0;
   explosion           = hegrenadeExplosion;
   muzzleVelocity      = 24;
   velInheritFactor    = 0;
   explodeOnDeath      = true;
   armingDelay         = 2000;
   lifetime            = 2000;
   fadeDelay           = 2500;
   bounceElasticity    = 0.3;
   bounceFriction      = 0.2;
   isBallistic         = true;
   gravityMod          = 1.0;
};

datablock ItemData(hegrenadeItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./grenade.dts";
   uiName = "Stand-in Grenade";
   image = hegrenadeImage;
   canDrop = true;
};

datablock ShapeBaseImageData(hegrenadeImage)
{
   shapeFile = "./grenade.dts";
   mountPoint = 0;
   correctMuzzleVector = true;
   className = "WeaponImage";
   item = hegrenadeItem;
   projectile = hegrenadeProjectile;
   projectileType = Projectile;
   casing = hegrenadePinDebris;
   shellExitDir      = "-1.0 1.0 1.0";
   shellVelocity     = 5.0;
   armReady = true;

   stateName[0]                    = "Activate";
   stateTimeoutValue[0]            = 0.2;
   stateTransitionOnTimeout[0]     = "Ready";

   stateName[1]                    = "Ready";
   stateTransitionOnTriggerDown[1] = "Pindrop";
   stateAllowImageChange[1]        = true;

   stateName[2]                    = "Pindrop";
   stateTransitionOnTimeout[2]     = "Pinfallen";
   stateTimeoutValue[2]            = 0.3;
   stateEjectShell[2]              = true;
   stateAllowImageChange[2]        = false;

   stateName[3]                    = "Pinfallen";
   stateTransitionOnTriggerDown[3] = "Charge";
   stateAllowImageChange[3]        = false;

   stateName[4]                    = "Charge";
   stateTransitionOnTimeout[4]     = "Armed";
   stateTimeoutValue[4]            = 0.5;
   stateWaitForTimeout[4]          = false;
   stateTransitionOnTriggerUp[4]   = "AbortCharge";
   stateScript[4]                  = "onCharge";
   stateAllowImageChange[4]        = false;

   stateName[5]                    = "AbortCharge";
   stateTransitionOnTimeout[5]     = "Pinfallen";
   stateTimeoutValue[5]            = 0.2;
   stateWaitForTimeout[5]          = true;
   stateScript[5]                  = "onAbortCharge";
   stateAllowImageChange[5]        = false;

   stateName[6]                    = "Armed";
   stateTransitionOnTriggerUp[6]   = "Fire";
   stateAllowImageChange[6]        = false;

   stateName[7]                    = "Fire";
   stateTransitionOnTimeout[7]     = "Done";
   stateTimeoutValue[7]            = 0.4;
   stateFire[7]                    = true;
   stateScript[7]                  = "onFire";
   stateWaitForTimeout[7]          = true;
   stateAllowImageChange[7]        = false;

   stateName[8]                    = "Done";
   stateScript[8]                  = "onDone";
};

package HEGrenadePackage
{
   function Armor::onCollision(%this, %obj, %col, %a, %b, %c, %d, %e, %f)
   {
      if(%col.dataBlock $= "HEGrenadeItem" && %col.canPickup)
      {
         for(%i = 0; %i < %this.maxTools; %i++)
         {
            %item = %obj.tool[%i];
            if(%item $= 0 || %item $= "")
            {
               %freeSlot = 1;
               break;
            }
         }
         if(%freeSlot)
         {
            %obj.pickup(%col);
            return;
         }
      }
      Parent::onCollision(%this, %obj, %col, %a, %b, %c, %d, %e, %f);
   }
};
activatePackage(HEGrenadePackage);

function hegrenadeImage::onCharge(%this, %obj, %slot)
{
   %obj.playThread(2, spearReady);
   %obj.lastHESlot = %obj.currTool;
}

function hegrenadeImage::onAbortCharge(%this, %obj, %slot)
{
   %obj.playThread(2, root);
}

function hegrenadeProjectile::onCollision(%this, %obj, %col, %fade, %pos, %normal)
{
   serverPlay3D(hegrenadeBounceSound, %obj.getTransform());
}

function hegrenadeImage::onFire(%this, %obj, %slot)
{
   %obj.playThread(2, spearThrow);
   Parent::onFire(%this, %obj, %slot);

   %currSlot = %obj.lastHESlot;
   %obj.tool[%currSlot] = 0;
   %obj.weaponCount--;
   messageClient(%obj.client, 'MsgItemPickup', '', %currSlot, 0);
   serverCmdUnUseTool(%obj.client);
}

function hegrenadeImage::onDone(%this, %obj, %slot)
{
   %obj.unMountImage(%slot);
}
