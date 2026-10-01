// Stand-in for the Weapon_ModernWarbattles port tests (CC0): the hl2 ammo
// system's shape, with our own guns, names and numbers.
AddDamageType("StandinPistol", '%1 shot themselves', '%2 shot %1', 0.75, 1);
AddDamageType("StandinShotgun", '%1 shot themselves', '%2 shot %1', 0.75, 1);
AddDamageType("StandinCrit", '%1 shot themselves', '%2 hit %1 hard', 0.75, 1);
AddDamageType("StandinClub", '%1 clubbed themselves', '%2 clubbed %1', 0.75, 1);

datablock AudioProfile(standinClubSoundA)
{
   filename    = "./hit.wav";
   description = AudioClose3d;
   preload     = true;
};

datablock AudioProfile(standinClubSoundB : standinClubSoundA)
{
};

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
   raycastCritDirectDamageType = $DamageType::StandinCrit;
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

datablock ProjectileData(shrapGrenTrailProjectile : standinPistolProjectile)
{
   directDamage        = 0;
   lifetime            = 400;
};

datablock ProjectileData(shrapGrenProjectile : standinPistolProjectile)
{
   directDamage        = 0;
   lifetime            = 3000;
   explodeOnDeath      = true;
};

datablock ItemData(shrapGrenItem : standinPistolItem)
{
   uiName = "Stand-in Grenade";
   image = shrapGrenImage;
   maxmag = 1;
   ammotype = "Frag Grenades";
};

// The pin drops as the trigger goes down; it flies as it comes up.
datablock ShapeBaseImageData(shrapGrenImage)
{
   shapeFile = "./pistol.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = shrapGrenItem;
   projectile = shrapGrenProjectile;
   projectileType = Projectile;

   stateName[0]                     = "Activate";
   stateTimeoutValue[0]             = 0.1;
   stateTransitionOnTimeout[0]      = "Ready";

   stateName[1]                     = "Ready";
   stateTransitionOnTriggerDown[1]  = "Tick";
   stateTransitionOnNoAmmo[1]       = "Empty";
   stateAllowImageChange[1]         = true;

   stateName[2]                     = "Tick";
   stateTransitionOnTriggerUp[2]    = "Fire";
   stateScript[2]                   = "onPinDrop";
   stateAllowImageChange[2]         = true;

   stateName[3]                     = "Fire";
   stateTransitionOnTimeout[3]      = "Activate";
   stateTimeoutValue[3]             = 0.2;
   stateFire[3]                     = true;
   stateScript[3]                   = "onFire";
   stateWaitForTimeout[3]           = true;

   stateName[4]                     = "Empty";
   stateTransitionOnAmmo[4]         = "Ready";
};

datablock ProjectileData(heavyMachineGunProjectile : standinPistolProjectile)
{
   directDamage        = 12;
};

datablock ItemData(heavyMachineGunItem : standinPistolItem)
{
   uiName = "Stand-in Heavy Gun";
   image = heavyMachineGunImage;
   maxmag = 30;
   ammotype = "Heavy Machine Gun";
};

// Held down, each shot spreads wider than the last, to the third.
datablock ShapeBaseImageData(heavyMachineGunImage : standinPistolImage)
{
   item = heavyMachineGunItem;
   projectile = heavyMachineGunProjectile;
   stateTransitionOnTimeout[2]      = "Fire1";
   stateTimeoutValue[2]             = 0.1;

   stateName[7]                     = "Fire1";
   stateTransitionOnTimeout[7]      = "Fire2";
   stateTransitionOnTriggerUp[7]    = "LoadCheckA";
   stateTransitionOnNoAmmo[7]       = "LoadCheckA";
   stateTimeoutValue[7]             = 0.1;
   stateFire[7]                     = true;
   stateScript[7]                   = "onFire2";
   stateWaitForTimeout[7]           = true;

   stateName[8]                     = "Fire2";
   stateTransitionOnTimeout[8]      = "Fire2";
   stateTransitionOnTriggerUp[8]    = "LoadCheckA";
   stateTransitionOnNoAmmo[8]       = "LoadCheckA";
   stateTimeoutValue[8]             = 0.1;
   stateFire[8]                     = true;
   stateScript[8]                   = "onFire3";
   stateWaitForTimeout[8]           = true;
};

function heavyMachineGunImage::onFire(%this, %obj, %slot)
{
   %projectile = %this.projectile;
   %spread = 0.001;
   %shellcount = 1;
   %obj.setVelocity(VectorAdd(%obj.getVelocity(), VectorScale(%obj.getEyeVector(), "-1")));
   for(%i = 0; %i < %shellcount; %i++)
      %p = new Projectile() { dataBlock = %projectile; scale = "2 2 2"; };
}

function heavyMachineGunImage::onFire2(%this, %obj, %slot)
{
   %obj.playThread(2, shiftRight);
   %obj.playThread(3, shiftLeft);
   %projectile = %this.projectile;
   %spread = 0.002;
   %shellcount = 1;
   %obj.setVelocity(VectorAdd(%obj.getVelocity(), VectorScale(%obj.getEyeVector(), "-0.5")));
   for(%i = 0; %i < %shellcount; %i++)
      %p = new Projectile() { dataBlock = %projectile; scale = "2 2 2"; };
}

function heavyMachineGunImage::onFire3(%this, %obj, %slot)
{
   %obj.playThread(2, shiftRight);
   %obj.playThread(3, shiftLeft);
   %projectile = %this.projectile;
   %spread = 0.003;
   %shellcount = 1;
   %obj.setVelocity(VectorAdd(%obj.getVelocity(), VectorScale(%obj.getEyeVector(), "-0.5")));
   for(%i = 0; %i < %shellcount; %i++)
      %p = new Projectile() { dataBlock = %projectile; scale = "2 2 2"; };
}

function heavyMachineGunProjectile::damage(%this, %obj, %col, %fade, %pos, %normal)
{
   %col.damage(%obj, %pos, %this.directDamage, %this.directDamageType);
}

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

function BatonImage::onFire(%this, %obj, %slot)
{
   if(getRandom(0, 1))
   {
      %this.raycastExplosionBrickSound = standinClubSoundA;
      %this.raycastExplosionPlayerSound = standinClubSoundA;
   }
   else
   {
      %this.raycastExplosionBrickSound = standinClubSoundB;
      %this.raycastExplosionPlayerSound = standinClubSoundB;
   }
   WeaponImage::onFire(%this, %obj, %slot);
}

function BatonImage::onRaycastDamage(%this, %obj, %slot, %col, %pos, %normal, %shotVec, %crit)
{
   %damage = %col.dataBlock.maxDamage * 2;
   %damageType = $DamageType::StandinClub;
   %col.setVelocity(vectorAdd(%col.getVelocity(), vectorAdd(vectorScale(%obj.getForwardVector(), 12), "0 0 6")));
   %col.damage(%obj, %pos, %damage, %damageType);
}

function MacheteImage::onFire(%this, %obj, %slot)
{
   if(getRandom(0, 1))
   {
      %this.raycastExplosionBrickSound = standinClubSoundA;
      %this.raycastExplosionPlayerSound = standinClubSoundA;
   }
   else
   {
      %this.raycastExplosionBrickSound = standinClubSoundB;
      %this.raycastExplosionPlayerSound = standinClubSoundB;
   }
   WeaponImage::onFire(%this, %obj, %slot);
}

function MacheteImage::onRaycastDamage(%this, %obj, %slot, %col, %pos, %normal, %shotVec, %crit)
{
   %damage = %col.dataBlock.maxDamage * 2;
   %damageType = $DamageType::StandinClub;
   %col.setVelocity(vectorAdd(%col.getVelocity(), vectorAdd(vectorScale(%obj.getForwardVector(), 12), "0 0 6")));
   %col.damage(%obj, %pos, %damage, %damageType);
}

function shrapGrenprojectile::onExplode(%this, %obj)
{
   Parent::onExplode(%this, %obj);
   %shrapData = ShrapGrenClusterProjectile;
   %shards = 4;
   for(%i = 0; %i < %shards; %i++)
      spawnShard(%shrapData, %obj.getPosition(), getRandom(-6,6), getRandom(-6,6), getRandom(-6,6));
   %shrapData = ShrapGrenTrailProjectile;
   %shards = 3;
   for(%i = 0; %i < %shards; %i++)
      spawnShard(%shrapData, %obj.getPosition(), getRandom(-12,12), getRandom(-12,12), getRandom(-12,12));
}

function shrapGrenImage::onPinDrop(%this, %obj, %slot)
{
   %obj.chargeStart = getSimTime();
   if(!isEventPending(%obj.burnSched))
   {
      %obj.burnSched = schedule(4000,0,"burnedIt",%obj,%slot);
      %obj.warnTime = "4 Seconds";
      %obj.warnSched = schedule(100,0,sendCenterNade,%obj.client);
   }
}

function sendCenterNade(%client)
{
   if(isObject(%client.player) && %client.player.warntime !$= "0 seconds")
   {
      commandtoclient(%client,'centerprint',"\c5"@%client.player.warnTime@"\c6 cooking time left.",0.15);
      %client.player.warntime = getWord(%client.player.warntime,0)-0.1@" seconds";
      if(getWord(%client.player.warnTime,0) == 1)
         %client.player.warnTime = "1 second";
      %client.player.warnsched = schedule(100,0,sendCenterNade,%client);
   }
}

function shrapGrenImage::onFire(%this, %obj, %slot)
{
   cancel(%obj.burnSched);
   cancel(%obj.warnSched);
   %obj.chargeEnd = getSimTime();
   %obj.chargeTime = %obj.chargeEnd - %obj.chargeStart;
   Parent::onFire(%this, %obj, %slot);
}

function burnedIt(%player, %slot)
{
   cancel(%player.warnSched);
   %pos = %player.getPosition();
   %posz = getWord(%pos,2);
   %burned = new Projectile()
   {
      datablock = shrapGrenProjectile;
      initialPosition = getWords(%pos,0,1) SPC %posz + 2;
      sourceObject = %player;
      isBurned = true;
   };
   %burned.explode();
}

package standinGrenadeFuse
{
   function projectile::onAdd(%obj,%a,%b)
   {
      parent::onAdd(%obj,%a,%b);
      if(%obj.dataBlock $= "shrapGrenProjectile" && !%obj.isBurned)
      {
         %oldLT = 4000;
         %cookTime = %obj.client.player.chargeTime;
         %new = %oldLT - %cookTime;
         %obj.schedule(%new,"explode");
      }
      if(%obj.dataBlock $= "shrapGrenClusterProjectile" && !%obj.isBurned)
      {
         %oldLT = 400;
         %new = %oldLT - (getRandom(0,400));
         %obj.schedule(%new,"explode");
      }
      if(%obj.dataBlock $= "shrapGrenTrailProjectile" && !%obj.isBurned)
      {
         %oldLT = 400;
         %new = %oldLT - (getRandom(100,400));
         %obj.schedule(%new,"explode");
      }
   }
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
