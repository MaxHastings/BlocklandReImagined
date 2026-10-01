// Stand-in for the Weapon_AdventurePack port tests (CC0): the Glass
// release's script shapes, with our own guns, names and numbers.
AddDamageType("StandinGun", '%1 shot themselves', '%2 shot %1', 0.75, 1);

datablock AudioProfile(standinFireSound)
{
   filename    = "./fire.wav";
   description = AudioClose3d;
   preload     = true;
};

datablock ExplosionData(standinKickExplosion)
{
   lifetimeMS       = 150;
   shakeCamera      = true;
   camShakeFreq     = "2 3 4";
   camShakeAmp      = "0.5 0.9 0.7";
   camShakeDuration = 0.4;
   camShakeRadius   = 10;
};

datablock ProjectileData(standinKickProjectile)
{
   explosion      = standinKickExplosion;
   lifetime       = 10;
   explodeOnDeath = true;
};

datablock ProjectileData(standinBulletProjectile)
{
   directDamage        = 10;
   directDamageType    = $DamageType::StandinGun;
   muzzleVelocity      = 90;
   velInheritFactor    = 1;
   lifetime            = 2000;
   isBallistic         = false;
   gravityMod          = 0.0;
   headshotMultiplier  = 2;
};

datablock ProjectileData(pairedShotgunProjectile : standinBulletProjectile)
{
   directDamage        = 5;
   headshotMultiplier  = 1.25;
};

datablock ProjectileData(pairedShotgunBlastProjectile : standinBulletProjectile)
{
   directDamage        = 30;
   headshotMultiplier  = 2;
};

datablock ProjectileData(taserProjectile : standinBulletProjectile)
{
   directDamage        = 50;
   headshotMultiplier  = 1.5;
};

datablock ItemData(standinPistolItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./pistol.dts";
   uiName = "Stand-in Pistol";
   image = standinPistolImage;
   canDrop = true;
   maxmag = 10;
   ammotype = "Pistol";
   reload = true;
   nochamber = 1;
};

datablock ShapeBaseImageData(standinPistolImage)
{
   shapeFile = "./pistol.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = standinPistolItem;
   projectile = standinBulletProjectile;
   projectileType = Projectile;
   armReady = true;

   stateName[0]                     = "Activate";
   stateTimeoutValue[0]             = 0.1;
   stateTransitionOnTimeout[0]      = "LoadCheck";

   stateName[1]                     = "Ready";
   stateTransitionOnTriggerDown[1]  = "Fire";
   stateTransitionOnNoAmmo[1]       = "Reload";
   stateAllowImageChange[1]         = true;

   stateName[2]                     = "Fire";
   stateTransitionOnTimeout[2]      = "LoadCheck";
   stateTimeoutValue[2]             = 0.2;
   stateFire[2]                     = true;
   stateScript[2]                   = "onFire";
   stateWaitForTimeout[2]           = true;

   stateName[3]                     = "LoadCheck";
   stateTransitionOnAmmo[3]         = "Ready";
   stateTransitionOnNoAmmo[3]       = "Reload";

   stateName[4]                     = "Reload";
   stateScript[4]                   = "onReload";
   stateTimeoutValue[4]             = 1.0;
   stateTransitionOnTimeout[4]      = "LoadCheck";
   stateWaitForTimeout[4]           = true;
};

datablock ItemData(pairedShotgunItem : standinPistolItem)
{
   shapeFile = "./shotgun.dts";
   uiName = "Stand-in Paired Shotgun";
   image = pairedShotgunImage;
   maxmag = 6;
   ammotype = "Shotgun";
};

datablock ShapeBaseImageData(pairedShotgunImage : standinPistolImage)
{
   shapeFile = "./shotgun.dts";
   item = pairedShotgunItem;
   projectile = pairedShotgunProjectile;
   stateScript[4]                   = "onReloadSingle";
   stateTimeoutValue[4]             = 0.5;
   stateTransitionOnTimeout[4]      = "CheckChamber";
   stateName[5]                     = "CheckChamber";
   stateTimeoutValue[5]             = 0.25;
   stateTransitionOnTimeout[5]      = "Reload";
   stateTransitionOnAmmo[5]         = "Ready";
};

datablock ItemData(sniperRifleItem : standinPistolItem)
{
   shapeFile = "./sniper.dts";
   uiName = "Stand-in Sniper Rifle";
   image = sniperRifleImage2;
   maxmag = 3;
   ammotype = "Sniper Rifle";
};

datablock ShapeBaseImageData(sniperRifleImage2 : standinPistolImage)
{
   shapeFile = "./sniper.dts";
   item = sniperRifleItem;
   directDamage = 40;
   directDamageType = $DamageType::StandinGun;
   headshotMultiplier = 2;
   raycastEnabled = 1;
   raycastRange = 300;
   raycastHitExplosion = "standinBulletProjectile";
   raycastTracer = StandinTracerShape;
};

datablock ItemData(taserItem : standinPistolItem)
{
   shapeFile = "./taser.dts";
   uiName = "Stand-in Taser";
   image = taserImage;
   maxmag = 1;
   ammotype = "Taser";
};

datablock ShapeBaseImageData(taserImage : standinPistolImage)
{
   shapeFile = "./taser.dts";
   item = taserItem;
   projectile = taserProjectile;
};

datablock ItemData(standinAmmoItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./box.dts";
   uiName = "Ammo [ALL]";
   image = standinPistolImage;
   canDrop = true;
   ammoBox = true;
   ammotype = "ALL";
};

datablock ItemData(standinAmmoPistolItem : standinAmmoItem)
{
   uiName = "Ammo [Pistol]";
   ammotype = "Pistol";
};

function hl2AmmoOnReload(%this, %obj, %slot)
{
   %item = %this.item;
   %need = %item.maxmag - %obj.toolMag[%obj.currTool];
   %have = %obj.toolAmmo[%item.ammotype];
   %move = %need < %have ? %need : %have;
   %obj.toolMag[%obj.currTool] += %move;
   %obj.toolAmmo[%item.ammotype] -= %move;
}

function standinPistolImage::onFire(%this, %obj, %slot)
{
   %projectile = %this.projectile;
   %spread = 0.0002;
   %shellcount = 1;
   if (getSimTime() - %obj.lastFired > 500)
      %spread /= 2;
   %obj.lastFired = getSimTime();
   %obj.setVelocity(VectorAdd(%obj.getVelocity(), VectorScale(%aimVec, "-1")));
   %obj.spawnExplosion(standinKickProjectile, "1 1 1");
   for(%i = 0; %i < %shellcount; %i++)
      fireOne(%this, %obj, %slot, %projectile, %spread);
   %obj.playThread(2, shiftRight);
   serverPlay3d(standinFireSound, %obj.getHackPosition());
}

// The reload's moves and sounds, timed by hand.
function standinPistolImage::onReload(%this, %obj, %slot)
{
   %obj.playThread(2, shiftUp);
   %obj.schedule(450, "playThread", "2", "plant");
   schedule(650, 0, serverPlay3D, standinFireSound, %obj.getHackPosition());
   hl2AmmoOnReload(%this, %obj, %slot);
}

// Two shells a pass, and a second tap a quarter second on.
function pairedShotgunImage::onReloadSingle(%this, %obj, %slot)
{
   %obj.playThread(2, shiftRight);
   serverPlay3d(standinFireSound, %obj.getHackPosition());
   hl2AmmoOnReloadSingle(%this, %obj, %slot);
   hl2AmmoOnReloadSingle(%this, %obj, %slot);
   %obj.schedule(250, "playThread", "2", "plant");
   schedule(250, 0, serverPlay3D, standinFireSound, %obj.getHackPosition());
}

function pairedShotgunImage::onFire(%this, %obj, %slot)
{
   if(%obj.toolMag[%obj.currTool] <= 2)
   {
      %projectile = %this.projectile;
      %spread = 0.002;
      %shellcount = 4;
      %obj.setVelocity(VectorAdd(%obj.getVelocity(), VectorScale(%aimVec, "-2")));
      for(%i = 0; %i < %shellcount; %i++)
         fireOne(%this, %obj, %slot, %projectile, %spread);

      %projectile = pairedShotgunBlastProjectile;
      %spread = 0.0005;
      %shellcount = 1;
      for(%i = 0; %i < %shellcount; %i++)
         fireOne(%this, %obj, %slot, %projectile, %spread);
   }
   if(%obj.toolMag[%obj.currTool] > 1)
   {
      %obj.toolMag[%obj.currTool] -= 2;
      %projectile = %this.projectile;
      %spread = 0.004;
      %shellcount = 8;
      %obj.setVelocity(VectorAdd(%obj.getVelocity(), VectorScale(%aimVec, "-4")));
      for(%i = 0; %i < %shellcount; %i++)
         fireOne(%this, %obj, %slot, %projectile, %spread);

      %projectile = pairedShotgunBlastProjectile;
      %spread = 0.0005;
      %shellcount = 1;
      for(%i = 0; %i < %shellcount; %i++)
         fireOne(%this, %obj, %slot, %projectile, %spread);
   }
}

function taserImage::onFire(%this, %obj, %slot)
{
   %projectile = %this.projectile;
   %spread = 0.0002;
   %shellcount = 1;
   %obj.setVelocity(VectorAdd(%obj.getVelocity(), VectorScale(%aimVec, "-1")));
   for(%i = 0; %i < %shellcount; %i++)
      fireOne(%this, %obj, %slot, %projectile, %spread);
}

package standinHeadshots
{
   function ProjectileData::damage(%this, %obj, %col, %fade, %pos, %normal)
   {
      if(%this.headshotMultiplier $= "")
         return Parent::damage(%this, %obj, %col, %fade, %pos, %normal);
      %damage = %this.directDamage;
      if(getHitbox(%obj, %col, %pos) $= "headSkin")
         %damage *= %this.headshotMultiplier;
      %col.damage(%obj, %pos, %damage, %this.directDamageType);
   }
};
activatePackage(standinHeadshots);

package standinRaycasts
{
   function WeaponImage::onFire(%this, %obj, %slot)
   {
      if(!%this.raycastEnabled || %this.raycastRange <= 0)
         return Parent::onFire(%this, %obj, %slot);
      castOne(%this, %obj, %slot);
   }
};
activatePackage(standinRaycasts);

function WeaponImage::damage(%this, %obj, %col, %pos, %normal, %vec)
{
   %col.damage(%obj, %pos, %this.directDamage, %this.directDamageType);
}

function taserProjectile::damage(%this, %obj, %col, %fade, %pos, %normal)
{
   if(isObject(%col.getMountedObject()))
      return;
   %col.addVelocity(getRandom(6) - 3 SPC getRandom(6) - 3 SPC 5);
   tumble(%col, 4000);
}

// The hitbox test: a head hit (or any hit on a crouched target) flinches the
// body for a moment.
function getHitbox(%obj, %col, %pos)
{
   if(!%col.isCrouched() && getWord(%pos, 2) < getWord(%col.getEyePoint(), 2) - 0.5)
      return "";
   %col.playThread(0, jump);
   %col.playThread(2, jump);
   %col.schedule(50, "playThread", "0", "plant");
   %col.schedule(50, "playThread", "2", "plant");
   return "headSkin";
}
