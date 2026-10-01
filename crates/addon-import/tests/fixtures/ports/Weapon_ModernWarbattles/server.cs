// Stand-in for the Weapon_ModernWarbattles port tests (CC0): the hl2 ammo
// system's shape, with our own guns, names and numbers.
AddDamageType("StandinPistol", '%1 shot themselves', '%2 shot %1', 0.75, 1);
AddDamageType("StandinShotgun", '%1 shot themselves', '%2 shot %1', 0.75, 1);

datablock ProjectileData(standinPistolProjectile)
{
   directDamage        = 10;
   directDamageType    = $DamageType::StandinPistol;
   muzzleVelocity      = 90;
   velInheritFactor    = 1;
   lifetime            = 2000;
   isBallistic         = false;
   gravityMod          = 0.0;
   headshotMultiplier  = 1.5;
};

datablock ProjectileData(standinShotgunProjectile)
{
   directDamage        = 8;
   directDamageType    = $DamageType::StandinShotgun;
   muzzleVelocity      = 90;
   velInheritFactor    = 1;
   lifetime            = 2000;
   isBallistic         = false;
   gravityMod          = 0.0;
   headshotMultiplier  = 1;
};

datablock ItemData(standinPistolItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./pistol.dts";
   uiName = "Stand-in Pistol";
   image = standinPistolImage;
   canDrop = true;
   maxmag = 12;
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
   projectile = standinPistolProjectile;
   projectileType = Projectile;
   armReady = true;

   stateName[0]                     = "Activate";
   stateTimeoutValue[0]             = 0.1;
   stateTransitionOnTimeout[0]      = "LoadCheckA";

   stateName[1]                     = "Ready";
   stateTransitionOnTriggerDown[1]  = "Fire";
   stateTransitionOnNoAmmo[1]       = "ReloadStart";
   stateAllowImageChange[1]         = true;

   stateName[2]                     = "Fire";
   stateTransitionOnTimeout[2]      = "LoadCheckA";
   stateTimeoutValue[2]             = 0.15;
   stateFire[2]                     = true;
   stateScript[2]                   = "onFire";
   stateWaitForTimeout[2]           = true;

   stateName[3]                     = "LoadCheckA";
   stateScript[3]                   = "onLoadCheck";
   stateTimeoutValue[3]             = 0.01;
   stateTransitionOnTimeout[3]      = "LoadCheckB";

   stateName[4]                     = "LoadCheckB";
   stateTransitionOnAmmo[4]         = "Ready";
   stateTransitionOnNoAmmo[4]       = "ReloadStart";

   stateName[5]                     = "ReloadStart";
   stateScript[5]                   = "onReloadStart";
   stateTimeoutValue[5]             = 0.5;
   stateTransitionOnTimeout[5]      = "Reload";
   stateWaitForTimeout[5]           = true;

   stateName[6]                     = "Reload";
   stateScript[6]                   = "onReload";
   stateTimeoutValue[6]             = 1.0;
   stateTransitionOnTimeout[6]      = "LoadCheckA";
   stateWaitForTimeout[6]           = true;
};

datablock ItemData(huntingShotgunItem : standinPistolItem)
{
   shapeFile = "./shotgun.dts";
   uiName = "Stand-in Shotgun";
   image = standinShotgunImage;
   maxmag = 5;
   ammotype = "Shotgun";
};

datablock ProjectileData(huntingShotgunBlastProjectile : standinShotgunProjectile)
{
   directDamage        = 20;
};

datablock ShapeBaseImageData(standinShotgunImage : standinPistolImage)
{
   shapeFile = "./shotgun.dts";
   item = huntingShotgunItem;
   projectile = standinShotgunProjectile;
   stateTransitionOnNoAmmo[1]       = "Reload";
   stateTransitionOnNoAmmo[4]       = "Reload";
   stateName[6]                     = "Reload";
   stateScript[6]                   = "onReloadSingle";
   stateTimeoutValue[6]             = 0.4;
   stateTransitionOnTimeout[6]      = "CheckChamber";
   stateWaitForTimeout[6]           = true;
   stateName[7]                     = "CheckChamber";
   stateTimeoutValue[7]             = 0.3;
   stateTransitionOnTimeout[7]      = "Reload";
   stateTransitionOnAmmo[7]         = "Ready";
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
};

datablock ItemData(standinAmmoPistolItem : standinAmmoItem)
{
   uiName = "Ammo [Pistol]";
   ammotype = "Pistol";
};

// Hitscan guns and melee swings, set up by image fields as a raycasting
// support script reads them.
datablock ItemData(revolverItem : standinPistolItem)
{
   uiName = "Stand-in Revolver";
   image = revolverImage;
   maxmag = 6;
   ammotype = "Revolver";
};

datablock ShapeBaseImageData(revolverImage : standinPistolImage)
{
   item = revolverItem;
   raycastWeaponRange = 200;
   raycastDirectDamage = 15;
   raycastDirectDamageType = $DamageType::StandinPistol;
   raycastExplosionProjectile = standinPistolProjectile;
};

datablock ItemData(BatonItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./baton.dts";
   uiName = "Stand-in Baton";
   image = BatonImage;
   canDrop = true;
};

datablock ShapeBaseImageData(BatonImage : standinPistolImage)
{
   item = BatonItem;
   raycastWeaponRange = 4;
   raycastDirectDamage = 10;
   raycastDirectDamageType = $DamageType::StandinPistol;
   raycastFromMuzzle = false;
};

// A frag grenade that bursts into shrapnel.
datablock ProjectileData(shrapGrenClusterProjectile : standinPistolProjectile)
{
   directDamage        = 12;
   lifetime            = 500;
};

datablock ProjectileData(shrapGrenProjectile : standinPistolProjectile)
{
   directDamage        = 0;
   lifetime            = 3000;
   explodeOnDeath      = true;
};

function standinPistolImage::onFire(%this, %obj, %slot)
{
   %projectile = %this.projectile;
   %spread = 0.0002;
   %shellcount = 1;
   %obj.setVelocity(VectorAdd(%obj.getVelocity(), VectorScale(%obj.getEyeVector(), "-1")));
   for(%i = 0; %i < %shellcount; %i++)
      fireOne(%this, %obj, %slot, %projectile, %spread);
}

function standinShotgunImage::onFire(%this, %obj, %slot)
{
   %projectile = %this.projectile;
   %spread = 0.004;
   %shellcount = 6;
   %obj.setVelocity(VectorAdd(%obj.getVelocity(), VectorScale(%obj.getEyeVector(), "-2")));
   for(%i = 0; %i < %shellcount; %i++)
      fireOne(%this, %obj, %slot, %projectile, %spread);

   %projectile = huntingShotgunBlastProjectile;
   %spread = 0.0005;
   %shellcount = 1;
   for(%i = 0; %i < %shellcount; %i++)
      fireOne(%this, %obj, %slot, %projectile, %spread);
}

function revolverImage::onFire(%this, %obj, %slot)
{
   %obj.setVelocity(VectorAdd(%obj.getVelocity(), VectorScale(%obj.getEyeVector(), "-3")));
   Parent::onFire(%this, %obj, %slot);
}

function WeaponImage::onRaycastDamage(%this, %obj, %slot, %col, %pos, %normal, %shotVec, %crit)
{
   %directDamage = mClampF(%this.raycastDirectDamage, -100, 100);
   if(%crit)
      %directDamage = %directDamage * 3;
   %col.damage(%obj, %pos, %directDamage, %this.raycastDirectDamageType);
}

function revolverImage::isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
   if(!(%col.getType() & $TypeMasks::PlayerObjectType))
      return 0;
   %col.setVelocity(vectorAdd(%col.getVelocity(), vectorAdd(vectorScale(%obj.getForwardVector(), 6), "0 0 4")));
   return getWord(%pos, 2) > getWord(%col.getWorldBoxCenter(), 2) - 3.3 * getWord(%col.getScale(), 2);
}

function battleRifleImage::isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
   if(!(%col.getType() & $TypeMasks::PlayerObjectType))
      return 0;
   %col.setVelocity(vectorAdd(%col.getVelocity(), vectorAdd(vectorScale(%obj.getForwardVector(), 7), "0 0 4")));
   return getWord(%pos, 2) > getWord(%col.getWorldBoxCenter(), 2) - 3.3 * getWord(%col.getScale(), 2);
}

function sniperrifleImage::isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
   if(!(%col.getType() & $TypeMasks::PlayerObjectType))
      return 0;
   %col.setVelocity(vectorAdd(%col.getVelocity(), vectorAdd(vectorScale(%obj.getForwardVector(), 15), "0 0 4")));
   return getWord(%pos, 2) > getWord(%col.getWorldBoxCenter(), 2) - 3.3 * getWord(%col.getScale(), 2);
}

function BatonImage::onRaycastDamage(%this, %obj, %slot, %col, %pos, %normal, %shotVec, %crit)
{
   %col.setVelocity(vectorAdd(%col.getVelocity(), vectorAdd(vectorScale(%obj.getForwardVector(), 12), "0 0 6")));
   %col.damage(%obj, %pos, %col.dataBlock.maxDamage * 2, %this.raycastDirectDamageType);
}

function MacheteImage::onRaycastDamage(%this, %obj, %slot, %col, %pos, %normal, %shotVec, %crit)
{
   %col.setVelocity(vectorAdd(%col.getVelocity(), vectorAdd(vectorScale(%obj.getForwardVector(), 12), "0 0 6")));
   %col.damage(%obj, %pos, %col.dataBlock.maxDamage * 2, %this.raycastDirectDamageType);
}

function shrapGrenprojectile::onExplode(%this, %obj)
{
   Parent::onExplode(%this, %obj);
   %shrapData = ShrapGrenClusterProjectile;
   %shards = 4;
   for(%i = 0; %i < %shards; %i++)
      spawnShard(%shrapData, %obj.getPosition(), getRandom(-6,6), getRandom(-6,6), getRandom(-6,6));
}

function hl2AmmoOnReload(%this, %obj, %slot)
{
   %item = %this.item;
   %need = %item.maxmag - %obj.toolMag[%obj.currTool];
   %have = %obj.toolAmmo[%item.ammotype];
   %move = %need < %have ? %need : %have;
   %obj.toolMag[%obj.currTool] += %move;
   %obj.toolAmmo[%item.ammotype] -= %move;
}

function standinPistolProjectile::damage(%this, %obj, %col, %fade, %pos, %normal)
{
   %damage = %this.directDamage;
   if(%col.isCrouched() || getHitbox(%obj, %col, %pos) $= "headSkin")
      %damage *= %this.headshotMultiplier;
   %col.damage(%obj, %pos, %damage, %this.directDamageType);
}

package standinAmmoSystem
{
   function Armor::onCollision(%this, %obj, %col, %vec, %speed)
   {
      if(%col.getDataBlock().ammoBox)
      {
         %obj.toolAmmo[%col.getDataBlock().ammotype] += 4;
         %obj.toolAmmo[%col.getDataBlock().ammotype] += 4;
         return;
      }
      Parent::onCollision(%this, %obj, %col, %vec, %speed);
   }
};
activatePackage(standinAmmoSystem);
