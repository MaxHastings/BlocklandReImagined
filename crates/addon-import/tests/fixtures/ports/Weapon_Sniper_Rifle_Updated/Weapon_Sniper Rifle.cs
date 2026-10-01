// Stand-in for the Weapon_Sniper_Rifle_Updated port tests (CC0): a self-contained
// rifle that draws its own hands: it hides the holder's and holds up both arms.
datablock ProjectileData(SniperRifleAnimatedProjectile)
{
   directDamage        = 90;
   muzzleVelocity      = 1500;
   velInheritFactor    = 1;
   lifetime            = 3000;
   fadeDelay           = 2500;
   isBallistic         = false;
   gravityMod          = 0.0;
};

datablock ItemData(SniperRifleAnimatedItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./sniperrifle.dts";
   uiName = "Stand-in Sniper Rifle Updated";
   image = SniperRifleAnimatedImage;
   canDrop = true;
};

datablock ShapeBaseImageData(SniperRifleAnimatedImage)
{
   shapeFile = "./sniperrifle.dts";
   mountPoint = 0;
   correctMuzzleVector = true;
   className = "WeaponImage";
   item = SniperRifleAnimatedItem;
   projectile = SniperRifleAnimatedProjectile;
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

function SniperRifleAnimatedImage::onFire(%this, %obj, %slot)
{
   %obj.playThread(2, plant);
   Parent::onFire(%this, %obj, %slot);
}

function SniperRifleAnimatedImage::onMount(%this, %obj, %slot)
{
   %obj.hideNode("lhand");
   %obj.hideNode("rhand");
   %obj.hideNode("lhook");
   %obj.hideNode("rhook");
   %obj.playThread(2, "armReadyBoth");
}

function SniperRifleAnimatedImage::onUnMount(%this, %obj, %slot)
{
   %client = %obj.client;
   if(!isObject(%client))
      return;
   %client.applyBodyParts();
   %client.applyBodyColors();
   %obj.playThread(2, "root");
}
