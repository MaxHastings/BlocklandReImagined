datablock AudioProfile(blasterChargeSound)
{
   filename    = "./charge.wav";
   description = AudioClose3d;
   preload = true;
};

AddDamageType("SyntheticBlaster", '<bitmap:add-ons/Weapon_Synthetic_Blaster/ci_blaster> %1', '%2 <bitmap:add-ons/Weapon_Synthetic_Blaster/ci_blaster> %1', 0.5, 1);

datablock ProjectileData(blasterBoltProjectile)
{
   projectileShapeName = "./bolt.dts";
   directDamage        = 12;
   directDamageType    = $DamageType::SyntheticBlaster;
   explosion           = blasterExplosion;
   muzzleVelocity      = 90;
   velInheritFactor    = 1;
   lifetime            = 2000;
   fadeDelay           = 1500;
   isBallistic         = false;
   gravityMod          = 0.0;
};

datablock ExplosionData(blasterExplosion)
{
   lifetimeMS   = 150;
   radiusDamage = 5;
   damageRadius = 1.5;
   emitter[0]   = blasterSparkEmitter;
};

datablock ItemData(blasterItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./blaster.dts";
   uiName = "Synthetic Blaster";
   iconName = "./icon_blaster";
   doColorShift = true;
   colorShiftColor = "0.2 0.4 0.9 1";
   image = blasterImage;
   canDrop = true;
};

datablock ShapeBaseImageData(blasterImage)
{
   shapeFile = "./blaster.dts";
   mountPoint = 0;
   offset = "0 0 0";
   rotation = eulerToMatrix( "0 0 0" );
   correctMuzzleVector = true;
   className = "WeaponImage";
   item = blasterItem;
   projectile = blasterBoltProjectile;
   projectileType = Projectile;
   armReady = true;
   doColorShift = true;
   colorShiftColor = blasterItem.colorShiftColor;

   stateName[0]                    = "Activate";
   stateTimeoutValue[0]            = 0.1;
   stateTransitionOnTimeout[0]     = "Ready";

   stateName[1]                    = "Ready";
   stateTransitionOnTriggerDown[1] = "Fire";
   stateAllowImageChange[1]        = true;

   stateName[2]                    = "Fire";
   stateTransitionOnTimeout[2]     = "Ready";
   stateTimeoutValue[2]            = 0.2;
   stateFire[2]                    = true;
   stateScript[2]                  = "onFire";
   stateWaitForTimeout[2]          = true;
   stateSound[2]                   = blasterChargeSound;
};

// Fires a burst of three bolts with a little spread and pushes the shooter back.
function blasterImage::onFire(%this, %obj, %slot)
{
   %obj.lastBlasterShot = getSimTime();
   %obj.setVelocity(VectorAdd(%obj.getVelocity(), VectorScale(%obj.getEyeVector(), "-2")));
   for(%i = 0; %i < 3; %i++)
   {
      %velocity = VectorScale(%obj.getMuzzleVector(%slot), %this.projectile.muzzleVelocity);
      %velocity = VectorAdd(%velocity, (getRandom() - 0.5) SPC (getRandom() - 0.5) SPC 0);
      %p = new (%this.projectileType)()
      {
         dataBlock = %this.projectile;
         initialVelocity = %velocity;
         initialPosition = %obj.getMuzzlePoint(%slot);
         sourceObject = %obj;
         sourceSlot = %slot;
         client = %obj.client;
      };
      MissionCleanup.add(%p);
   }
   messageClient(%obj.client, '', "Blaster burst!");
}

function blasterImage::onMount(%this, %obj, %slot)
{
   // Intentionally empty.
}

package SyntheticBlasterPackage
{
   function Armor::onCollision(%this, %obj, %col, %vec, %speed)
   {
      if(%col.getDataBlock() == blasterItem.getId())
         eval("%obj.blasterPickups++;");
      Parent::onCollision(%this, %obj, %col, %vec, %speed);
   }
};
activatePackage(SyntheticBlasterPackage);
