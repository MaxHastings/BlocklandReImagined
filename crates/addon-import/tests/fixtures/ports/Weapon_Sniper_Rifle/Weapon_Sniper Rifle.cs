// Stand-in for the Weapon_Sniper_Rifle port tests (CC0): a self-contained
// rifle whose onFire kicks the arm on thread 2 while the shooter lives.
datablock ProjectileData(SniperRifleProjectile)
{
   directDamage        = 90;
   muzzleVelocity      = 1500;
   velInheritFactor    = 1;
   lifetime            = 3000;
   fadeDelay           = 2500;
   isBallistic         = false;
   gravityMod          = 0.0;
};

datablock ItemData(SniperRifleItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./sniperrifle.dts";
   uiName = "Stand-in Sniper Rifle";
   image = SniperRifleImage;
   canDrop = true;
};

datablock ShapeBaseImageData(SniperRifleImage)
{
   shapeFile = "./sniperrifle.dts";
   mountPoint = 0;
   correctMuzzleVector = true;
   className = "WeaponImage";
   item = SniperRifleItem;
   projectile = SniperRifleProjectile;
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
   stateTransitionOnTimeout[2]     = "Smoke";
   stateTimeoutValue[2]            = 0.1;
   stateFire[2]                    = true;
   stateAllowImageChange[2]        = false;
   stateScript[2]                  = "onFire";
   stateWaitForTimeout[2]          = true;

   stateName[3]                    = "Smoke";
   stateTimeoutValue[3]            = 1.0;
   stateTransitionOnTimeout[3]     = "Reload";
   stateWaitForTimeout[3]          = true;

   stateName[4]                    = "Reload";
   stateTransitionOnTriggerUp[4]   = "Ready";
};

function SniperRifleImage::onFire(%this, %obj, %slot)
{
   if(%obj.getDamagePercent() < 1.0)
      %obj.playThread(2, shiftAway);
   Parent::onFire(%this, %obj, %slot);
}
