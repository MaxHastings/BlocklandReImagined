// Stand-in for the Weapon_Shotgun port tests (CC0): the widespread v20
// "spread" onFire shape, with its own numbers, on a self-contained weapon.
datablock ProjectileData(shotgunProjectile)
{
   directDamage        = 10;
   muzzleVelocity      = 80;
   velInheritFactor    = 1;
   lifetime            = 1000;
   fadeDelay           = 900;
   isBallistic         = false;
   gravityMod          = 0.0;
};

datablock ItemData(shotgunItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./shotgun.dts";
   uiName = "Stand-in Shotgun";
   image = shotgunImage;
   canDrop = true;
};

datablock ShapeBaseImageData(shotgunImage)
{
   shapeFile = "./shotgun.dts";
   mountPoint = 0;
   correctMuzzleVector = true;
   className = "WeaponImage";
   item = shotgunItem;
   projectile = shotgunProjectile;
   projectileType = Projectile;
   armReady = true;
   minShotTime = 600;

   stateName[0]                    = "Activate";
   stateTimeoutValue[0]            = 0.1;
   stateTransitionOnTimeout[0]     = "Ready";

   stateName[1]                    = "Ready";
   stateTransitionOnTriggerDown[1] = "Fire";
   stateAllowImageChange[1]        = true;

   stateName[2]                    = "Fire";
   stateTransitionOnTimeout[2]     = "Ready";
   stateTimeoutValue[2]            = 0.6;
   stateFire[2]                    = true;
   stateScript[2]                  = "onFire";
   stateWaitForTimeout[2]          = true;
};

function shotgunImage::onFire(%this, %obj, %slot)
{
   if((%obj.lastFireTime + %this.minShotTime) > getSimTime())
      return;
   %obj.lastFireTime = getSimTime();

   %obj.setVelocity(VectorAdd(%obj.getVelocity(), VectorScale(%obj.getEyeVector(), "-4")));
   %obj.playThread(2, shiftAway);

   %projectile = %this.projectile;
   %spread = 0.002;
   %shellcount = 5;

   for(%shell = 0; %shell < %shellcount; %shell++)
   {
      %vector = %obj.getMuzzleVector(%slot);
      %objectVelocity = %obj.getVelocity();
      %vector1 = VectorScale(%vector, %projectile.muzzleVelocity);
      %vector2 = VectorScale(%objectVelocity, %projectile.velInheritFactor);
      %velocity = VectorAdd(%vector1, %vector2);
      %x = (getRandom() - 0.5) * 10 * 3.1415926 * %spread;
      %y = (getRandom() - 0.5) * 10 * 3.1415926 * %spread;
      %z = (getRandom() - 0.5) * 10 * 3.1415926 * %spread;
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
