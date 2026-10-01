// Stand-in for the Weapon_Package_Tier1 port tests (CC0): the shape of the
// Tier+Tactical scripts its port reads, with our own guns, names and numbers.
AddDamageType("StandinSidearm", '%1 shot themselves', '%2 shot %1', 0.75, 1);
AddDamageType("StandinRifle", '%1 shot themselves', '%2 shot %1', 0.75, 1);
AddDamageType("StandinRifleHeadshot", '%1 shot themselves', '%2 headshot %1', 0.75, 1);
AddDamageType("StandinPellet", '%1 shot themselves', '%2 shot %1', 0.75, 1);

// Its preferences, in RTB's server control when it is there.
if($RTB::Hooks::ServerControl)
{
   RTB_registerPref("Starting 9mm","Stand-in | Starting Ammo","$Pref::Server::TT::Start9MM","int 0 280","Weapon_Package_Tier1",35*4,0,1);
   RTB_registerPref("Most 9mm","Stand-in | Maximum Ammo","$Pref::Server::TT::Max9MM","int 1 560","Weapon_Package_Tier1",280,0,1);
   RTB_registerPref("Players Drop Ammo","Stand-in | Ammo","$Pref::Server::TT::PlayerAmmoDrop","bool","Weapon_Package_Tier1",1,0,1);
   RTB_registerPref("Shake on Firing","Stand-in | Miscellaneous","$Pref::Server::TT::Recoil","bool","Weapon_Package_Tier1",1,0,1);
}
else
{
   TT_defaultIfUnset("Start9MM", 35*4);
   TT_defaultIfUnset("Max9MM", 280);
   TT_defaultIfUnset("PlayerAmmoDrop", 1);
   TT_defaultIfUnset("Recoil", 1);
}

// The ammo types this pack hands every player, as Kai's packs register them.
TT_registerAmmoType("9MM", "9mm", "9mm", true, "tt", "weps", "tt_pile");
TT_registerAmmoType("556", "5.56", "5.56 Little Rifle", true, "tt", "weps", "tt_pile");
TT_registerAmmoType("shotgun", "Buckshot", "12-gauge shotgun", true, "tt", "weps", "tt_pile");

datablock AudioProfile(standinClickSound)
{
   filename = "./click.wav";
   description = AudioClosest3d;
   preload = true;
};
datablock AudioProfile(standinBoomSound : standinClickSound) { filename = "./boom.wav"; };
datablock AudioProfile(standinJamSound : standinClickSound) { filename = "./jam.wav"; };
datablock AudioProfile(standinMoveSound : standinClickSound) { filename = "./move.wav"; };
datablock AudioProfile(ammoGetSound : standinClickSound) { filename = "./ammoget.wav"; };

// A sound pack used when the player has it, else the base game's click.
if(isFile("Add-Ons/Sound_Standin/server.cs"))
{
   ForceRequiredAddOn("Sound_Standin");
}
else
{
   datablock AudioProfile(Block_MoveBrick_Sound)
   {
      filename = "base/data/sound/clickMove.wav";
      description = AudioClosest3d;
      preload = false;
   };
}

datablock ExplosionData(standinKickExplosion)
{
   lifeTimeMS = 150;
   shakeCamera = true;
   camShakeFreq = "2 4 3";
   camShakeAmp = "0.2 0.4 0.3";
   camShakeDuration = 0.4;
   camShakeRadius = 10.0;
};

datablock ProjectileData(standinKickProjectile)
{
   lifetime = 10;
   fadeDelay = 10;
   explodeOnDeath = true;
   explosion = standinKickExplosion;
};

datablock ProjectileData(standinTracerProjectile)
{
   muzzleVelocity = 300;
   velInheritFactor = 0;
   lifetime = 300;
   isBallistic = false;
   gravityMod = 0.0;
};

datablock ProjectileData(standinSparkProjectile)
{
   muzzleVelocity = 1;
   lifetime = 10;
   explodeOnDeath = true;
   explosion = standinKickExplosion;
};

datablock ProjectileData(standinRifleProjectile)
{
   directDamage = 20;
   directDamageType = $DamageType::StandinRifle;
   muzzleVelocity = 150;
   velInheritFactor = 0;
   lifetime = 2000;
   isBallistic = false;
   gravityMod = 0.0;
};

datablock ProjectileData(standinRifleWeakProjectile : standinRifleProjectile)
{
   directDamage = 12;
};

datablock ProjectileData(standinSMGProjectile : standinRifleProjectile)
{
   directDamage = 4;
   directDamageType = $DamageType::StandinPellet;
};

datablock ProjectileData(standinPelletProjectile : standinSMGProjectile)
{
   directDamage = 3;
};

datablock ProjectileData(standinBlastProjectile : standinSMGProjectile)
{
   directDamage = 1;
   muzzleVelocity = 40;
   lifetime = 60;
   impactImpulse = 500;
};

// The raycasting sidearm.
datablock ItemData(standinSidearmItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./sidearm.dts";
   uiName = "Stand-in Sidearm";
   image = standinSidearmImage;
   canDrop = true;
   TT_reloads = true;
   TT_ammoType = "9MM";
   TT_maxAmmo = 6;
};

datablock ShapeBaseImageData(standinSidearmImage)
{
   shapeFile = "./sidearm.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = standinSidearmItem;
   projectile = standinTracerProjectile;
   projectileType = Projectile;
   armReady = true;

   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 150;
   TT_raycastDirectDamage = 10;
   TT_raycastDirectDamageType = $DamageType::StandinSidearm;
   TT_raycastExplosionProjectile = standinSparkProjectile;
   TT_raycastExplosionBrickSound = standinClickSound;
   TT_raycastExplosionPlayerSound = standinClickSound;
   TT_raycastSpreadAmt = 0.0005;
   TT_raycastSpreadCount = 1;
   TT_raycastTracerProjectile = standinTracerProjectile;
   TT_raycastFromMuzzle = true;

   stateName[0]                     = "Activate";
   stateTimeoutValue[0]             = 0.1;
   stateTransitionOnTimeout[0]      = "LoadCheckA";

   stateName[1]                     = "Ready";
   stateTransitionOnNotLoaded[1]    = "ManualReload";
   stateTransitionOnTriggerDown[1]  = "FireCheckA";

   stateName[2]                     = "FireCheckA";
   stateScript[2]                   = "TT_onFireCheck";
   stateTimeoutValue[2]             = 0.01;
   stateTransitionOnTimeout[2]      = "FireCheckB";

   stateName[3]                     = "FireCheckB";
   stateTransitionOnLoaded[3]       = "Fire";
   stateTransitionOnNotLoaded[3]    = "EmptyFire";

   stateName[4]                     = "Fire";
   stateFire[4]                     = true;
   stateScript[4]                   = "onFire";
   stateTimeoutValue[4]             = 0.05;
   stateTransitionOnTimeout[4]      = "Smoke";
   stateAllowImageChange[4]         = false;
   stateWaitForTimeout[4]           = true;

   stateName[5]                     = "Smoke";
   stateTransitionOnTriggerUp[5]    = "LoadCheckA";

   stateName[6]                     = "LoadCheckA";
   stateScript[6]                   = "TT_onLoadCheck";
   stateTimeoutValue[6]             = 0.01;
   stateTransitionOnTimeout[6]      = "LoadCheckB";

   stateName[7]                     = "LoadCheckB";
   stateTransitionOnLoaded[7]       = "Ready";
   stateTransitionOnNotLoaded[7]    = "Empty";

   stateName[8]                     = "ReloadWait";
   stateScript[8]                   = "onReloadWait";
   stateTimeoutValue[8]             = 0.3;
   stateTransitionOnTimeout[8]      = "ReloadStart";
   stateWaitForTimeout[8]           = true;

   stateName[9]                     = "ReloadStart";
   stateScript[9]                   = "onReloadStart";
   stateTimeoutValue[9]             = 0.3;
   stateTransitionOnTimeout[9]      = "Reloaded";
   stateWaitForTimeout[9]           = true;

   stateName[10]                    = "Reloaded";
   stateScript[10]                  = "onReloaded";
   stateTimeoutValue[10]            = 0.2;
   stateTransitionOnTimeout[10]     = "Ready";
   stateTransitionOnNoAmmo[10]      = "Empty";
   stateWaitForTimeout[10]          = true;

   stateName[11]                    = "ManualReload";
   stateTransitionOnAmmo[11]        = "ReloadWait";
   stateTransitionOnNoAmmo[11]      = "ReloadStart";

   stateName[12]                    = "Empty";
   stateTransitionOnLoaded[12]      = "Ready";
   stateTransitionOnAmmo[12]        = "ReloadWait";
   stateTransitionOnTriggerDown[12] = "FireCheckA";

   stateName[13]                    = "EmptyFire";
   stateScript[13]                  = "TT_onEmptyFire";
   stateTransitionOnLoaded[13]      = "Ready";
   stateTransitionOnAmmo[13]        = "ReloadWait";
   stateTransitionOnTriggerUp[13]   = "Empty";
};

function standinSidearmImage::onFire(%this,%obj,%slot)
{
   if($Pref::Server::TT::Recoil)
      %obj.spawnExplosion(standinKickProjectile,"1 1 1");

   if(vectorLen(%obj.getVelocity()) > 0.1)
   {
      %this.TT_raycastSpreadAmt = 0.002;
      %this.TT_raycastWeaponRange = 60;
   }
   else
   {
      %this.TT_raycastSpreadAmt = 0.0004;
      %this.TT_raycastWeaponRange = 150;
   }

   %this.TT_decrementAmmo(%obj);
   %this.TT_displayAmmo(%obj);
   %obj.playThread(2, shiftAway);
   return Parent::onFire(%this,%obj,%slot);
}

function standinSidearmImage::onReloadWait(%this,%obj,%slot)
{
   %obj.playThread(2, shiftUp);
   serverPlay3D(block_MoveBrick_Sound,%obj.getPosition());
}

function standinSidearmImage::onReloadStart(%this,%obj,%slot)
{
   %obj.playThread(2, shiftLeft);
}

function standinSidearmImage::onReloaded(%this,%obj,%slot)
{
   if($Pref::Server::TT::DeathStopAnims && %obj.getDamagePercent() >= 1.0)
      return %obj.setImageLoaded(%slot, 1);
   %this.TT_reload(%obj, %slot, standinClickSound, plant);
   %this.TT_displayAmmo(%obj);
}

// The pump, a shell at a time, with its blast.
datablock ItemData(standinPumpItem : standinSidearmItem)
{
   shapeFile = "./pump.dts";
   uiName = "Stand-in Pump";
   image = standinPumpImage;
   TT_ammoType = "shotgun";
   TT_maxAmmo = 3;
};

datablock ShapeBaseImageData(standinPumpImage)
{
   shapeFile = "./pump.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = standinPumpItem;
   projectile = standinPelletProjectile;
   projectileType = Projectile;
   armReady = true;

   stateName[0]                     = "Activate";
   stateTimeoutValue[0]             = 0.1;
   stateTransitionOnTimeout[0]      = "LoadCheckA";

   stateName[1]                     = "Ready";
   stateTransitionOnNotLoaded[1]    = "ReloadCheckA";
   stateTransitionOnTriggerDown[1]  = "Fire";

   stateName[2]                     = "Fire";
   stateFire[2]                     = true;
   stateScript[2]                   = "onFire";
   stateTimeoutValue[2]             = 0.1;
   stateTransitionOnTimeout[2]      = "Eject";
   stateAllowImageChange[2]         = false;
   stateWaitForTimeout[2]           = true;

   stateName[3]                     = "Eject";
   stateScript[3]                   = "onEject";
   stateTimeoutValue[3]             = 0.2;
   stateTransitionOnTimeout[3]      = "LoadCheckA";
   stateWaitForTimeout[3]           = true;

   stateName[4]                     = "LoadCheckA";
   stateScript[4]                   = "TT_onLoadCheck";
   stateTimeoutValue[4]             = 0.01;
   stateTransitionOnTimeout[4]      = "LoadCheckB";

   stateName[5]                     = "LoadCheckB";
   stateTransitionOnLoaded[5]       = "Ready";
   stateTransitionOnAmmo[5]         = "Reload";
   stateTransitionOnNoAmmo[5]       = "Empty";

   stateName[6]                     = "ReloadCheckA";
   stateScript[6]                   = "TT_onReloadCheck";
   stateTimeoutValue[6]             = 0.01;
   stateTransitionOnTimeout[6]      = "ReloadCheckB";

   stateName[7]                     = "ReloadCheckB";
   stateTransitionOnLoaded[7]       = "CompleteReload";
   stateTransitionOnNotLoaded[7]    = "Reload";

   stateName[8]                     = "Empty";
   stateTransitionOnLoaded[8]       = "Ready";
   stateTransitionOnAmmo[8]         = "Reload";

   stateName[9]                     = "Reload";
   stateScript[9]                   = "onReloadStart";
   stateTimeoutValue[9]             = 0.2;
   stateTransitionOnTimeout[9]      = "Reloaded";
   stateTransitionOnTriggerDown[9]  = "Fire";
   stateWaitForTimeout[9]           = false;

   stateName[10]                    = "Reloaded";
   stateScript[10]                  = "onReloaded";
   stateTimeoutValue[10]            = 0.1;
   stateTransitionOnTimeout[10]     = "ReloadCheckA";
   stateWaitForTimeout[10]          = true;

   stateName[11]                    = "CompleteReload";
   stateScript[11]                  = "onEject";
   stateTimeoutValue[11]            = 0.2;
   stateTransitionOnTimeout[11]     = "Ready";
   stateWaitForTimeout[11]          = true;
};

function standinPumpImage::onFire(%this,%obj,%slot)
{
   if(%this.TT_canFire(%obj))
   {
      serverPlay3D(standinBoomSound,%obj.getPosition());
      %obj.playThread(2, activate);

      %this.TT_decrementAmmo(%obj);

      if($Pref::Server::TT::Recoil)
         %obj.spawnExplosion(standinKickProjectile,"1 1 1");

      %projectile = %this.projectile;
      %spread = 0.004;
      %shellCount = 5;

      %p = TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
      TT_createProjectile(%this, %obj, %slot, standinBlastProjectile, 1);
   }
   else if(!$Pref::Server::TT::DeathStopFiring || %obj.getDamagePercent() < 1.0)
   {
      serverPlay3D(standinJamSound,%obj.getPosition());
   }
   %this.TT_displayAmmo(%obj);
   return %p;
}

function standinPumpImage::onEject(%this,%obj,%slot)
{
   %obj.playThread(2, plant);
}

function standinPumpImage::onReloadStart(%this,%obj,%slot)
{
   %obj.playThread(2, shiftto);
   serverPlay3D(standinMoveSound,%obj.getPosition());
}

function standinPumpImage::onReloaded(%this,%obj,%slot)
{
   %this.TT_incrementReload(%obj, %slot);
   %this.TT_displayAmmo(%obj);
}

// The rifle: a steady round standing, a weaker one on the move.
datablock ItemData(standinRifleItem : standinSidearmItem)
{
   shapeFile = "./rifle.dts";
   uiName = "Stand-in Rifle";
   image = standinRifleImage;
   TT_ammoType = "556";
   TT_maxAmmo = 4;
};

datablock ShapeBaseImageData(standinRifleImage : standinSidearmImage)
{
   shapeFile = "./rifle.dts";
   item = standinRifleItem;
   projectile = standinRifleProjectile;
   TT_raycastEnabled = false;
};

function standinRifleImage::onFire(%this,%obj,%slot)
{
   %obj.playThread(2, plant);

   if(vectorLen(%obj.getVelocity()) < 3 && (getSimTime() - %obj.lastShotTime) > 1000)
   {
      %projectile = %this.projectile;
      %spread = 0.0001;
   }
   else
   {
      %projectile = standinRifleWeakProjectile;
      %spread = 0.001;
   }
   %shellCount = 1;

   %this.TT_decrementAmmo(%obj);
   return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function standinRifleImage::onReloaded(%this,%obj,%slot)
{
   %this.TT_reload(%obj, %slot, standinClickSound, plant);
}

function standinRifleProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
   %multiplier = 2.5; // on a headshot
   TT_processHeadshotDamage(%this, %obj, %col, %pos, %this.directDamage, %multiplier, $DamageType::StandinRifleHeadshot);
}

function standinRifleWeakProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
   TT_processHeadshotDamage(%this, %obj, %col, %pos, %this.directDamage, 2, $DamageType::StandinRifleHeadshot);
}

// The submachine gun, whose bullets slow whoever they hit.
datablock ItemData(standinSMGItem : standinSidearmItem)
{
   shapeFile = "./smg.dts";
   uiName = "Stand-in SMG";
   image = standinSMGImage;
   TT_maxAmmo = 10;
};

datablock ShapeBaseImageData(standinSMGImage : standinSidearmImage)
{
   shapeFile = "./smg.dts";
   item = standinSMGItem;
   projectile = standinSMGProjectile;
   TT_raycastEnabled = false;
};

function standinSMGImage::onFire(%this,%obj,%slot)
{
   %projectile = %this.projectile;
   %spread = 0.002;
   %shellCount = 1;
   %obj.playThread(2, plant);
   %this.TT_decrementAmmo(%obj);
   return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function standinSMGImage::onReloaded(%this,%obj,%slot)
{
   %this.TT_reload(%obj, %slot, standinClickSound, plant);
}

function standinSMGProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
   if(%col.getType() & $TypeMasks::PlayerObjectType)
   {
      TT_dampenVelocity(%col, 2);
   }
   Parent::damage(%this,%obj,%col,%fade,%pos,%normal);
}

// A pair of sidearms from one count.
datablock ItemData(standinPairItem : standinSidearmItem)
{
   shapeFile = "./sidearm.dts";
   uiName = "Stand-in Pair";
   image = standinPairImage;
   TT_maxAmmo = 4;
};

datablock ShapeBaseImageData(standinPairImage : standinSidearmImage)
{
   item = standinPairItem;
   stateTransitionOnTimeout[5]      = "PairCheckA";
   stateTimeoutValue[5]             = 0.05;
   stateTransitionOnTriggerUp[5]    = "";

   stateName[14]                    = "PairCheckA";
   stateScript[14]                  = "TT_onFireCheck";
   stateTimeoutValue[14]            = 0.01;
   stateTransitionOnTimeout[14]     = "PairCheckB";

   stateName[15]                    = "PairCheckB";
   stateTransitionOnLoaded[15]      = "Pair";
   stateTransitionOnNotLoaded[15]   = "EmptyFire";

   stateName[16]                    = "Pair";
   stateScript[16]                  = "onFireAkimbo";
   stateTimeoutValue[16]            = 0.1;
   stateTransitionOnTimeout[16]     = "LoadCheckA";
};

datablock ShapeBaseImageData(standinLeftImage)
{
   shapeFile = "./sidearm.dts";
   mountPoint = 1;
   className = "WeaponImage";
   item = standinPairItem;
   projectile = standinTracerProjectile;
   projectileType = Projectile;

   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 150;
   TT_raycastDirectDamage = 10;
   TT_raycastDirectDamageType = $DamageType::StandinSidearm;
   TT_raycastTracerProjectile = standinTracerProjectile;
   TT_raycastFromMuzzle = true;

   stateName[0]                     = "Activate";
   stateTimeoutValue[0]             = 0.1;
   stateTransitionOnTimeout[0]      = "Ready";

   stateName[1]                     = "Ready";
   stateTransitionOnTriggerDown[1]  = "Fire";

   stateName[2]                     = "Fire";
   stateFire[2]                     = true;
   stateScript[2]                   = "onFire";
   stateTimeoutValue[2]             = 0.05;
   stateTransitionOnTimeout[2]      = "Ready";
};

function standinPairImage::onMount(%this, %obj, %slot)
{
   Parent::onMount(%this, %obj, %slot);
   %obj.mountImage(standinLeftImage, 1);
}

function standinPairImage::onFireAkimbo(%this,%obj,%slot)
{
   %obj.setImageTrigger(1,1);
}

function standinLeftImage::onFire(%this,%obj,%slot)
{
   if(vectorLen(%obj.getVelocity()) > 0.1)
   {
      %this.TT_raycastSpreadAmt = 0.003;
      %this.TT_raycastWeaponRange = 50;
   }
   else
   {
      %this.TT_raycastSpreadAmt = 0.001;
      %this.TT_raycastWeaponRange = 120;
   }
   %this.TT_decrementAmmo(%obj);
   return Parent::onFire(%this,%obj,%slot);
}

// Ammo lying about, and the bag a dead player's ammo spills into.
datablock ItemData(standinNineItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./ammo.dts";
   uiName = "Stand-in 9mm";
   canDrop = true;
   TT_ammoPickup = true;
   TT_ammoPickup[0] = "9mm 30";
};

function standinNineItem::onAdd(%this, %obj)
{
   %obj.rotate = true;
   %obj.setShapeName(getWord(%obj.TT_ammoPickup[0], 1));
   Parent::onAdd(%this, %obj);
}

datablock ItemData(standinPileItem : standinNineItem)
{
   uiName = "Stand-in Pile";
   TT_ammoPickup[0] = "9MM -1";
   TT_ammoPickup[1] = "shotgun 2";
};

datablock ItemData(ammoDroppedItem)
{
   shapeFile = "./bag.dts";
   uiName = "";
   canDrop = true;
};

// The ammo system's shape, as the port's covers read it: our own minimal
// versions of the support functions every Tier+Tactical pack calls.
package StandinAmmo
{
   function WeaponImage::TT_onLoadCheck(%this,%obj,%slot)
   {
      %obj.setImageLoaded(%slot, !%this.TT_needsAmmo(%obj));
      %obj.setImageAmmo(%slot, %this.TT_canStartReload(%obj));
   }

   function WeaponImage::TT_onReloadCheck(%this,%obj,%slot)
   {
      %toolNum = TT_getActiveTool(%obj);
      %can = %this.TT_canReload(%obj);
      %obj.setImageLoaded(%slot, !%can || !(%obj.TT_toolAmmo[%toolNum] < %this.item.TT_maxAmmo));
      %obj.setImageAmmo(%slot, %can);
   }

   function WeaponImage::TT_onFireCheck(%this,%obj,%slot)
   {
      %obj.setImageLoaded(%slot, %this.TT_canFire(%obj));
      %obj.setImageAmmo(%slot, %this.TT_canReload(%obj));
   }

   function WeaponImage::TT_onUseLight(%this, %obj)
   {
      %state = %obj.getImageState(0);
      if(%state $= "Ready")
         %obj.setImageLoaded(0, 0);
      else if(%state $= "Empty" || %state $= "EmptyFire")
         %obj.setImageAmmo(0, 1);
   }

   function WeaponImage::TT_reload(%this, %obj, %slot, %sound, %anim)
   {
      %obj.playThread(2, %anim);
      serverPlay3D(%sound, %obj.getPosition());
      %type = %this.item.TT_ammoType;
      %obj.quantity[%type] -= %this.item.TT_maxAmmo;
      %obj.setImageLoaded(%slot, 1);
   }

   function WeaponImage::TT_incrementReload(%this, %obj, %slot, %amount, %sound, %anim)
   {
      if(%amount $= "")
         %amount = 1;
      %obj.quantity[%this.item.TT_ammoType] -= %amount;
   }

   function Armor::onAdd(%this,%obj)
   {
      Parent::onAdd(%this,%obj);
      %obj.quantity["9MM"] = $Pref::Server::TT::Start["9MM"];
   }

   function Armor::onCollision(%this, %obj, %col, %vec, %force)
   {
      for(%i = 0; (%pickup = %col.TT_ammoPickup[%i]) !$= ""; %i++)
         %took += TT_addAmmo(%obj, getWord(%pickup, 0), getWord(%pickup, 1));
      if(%took)
         serverPlay3D(AmmoGetSound, %obj.getPosition());
   }

   function Armor::onDisabled(%this, %obj, %state)
   {
      %i = new Item() { datablock = ammoDroppedItem; };
      if(%type.canDrop)
         %i.TT_ammoPickup[%k] = %typeName SPC %ammoCount;
      %i.setVelocity(vectorAdd(%obj.getVelocity(), getRandom(-8, 8) SPC getRandom(-8, 8) SPC 4));
      Parent::onDisabled(%this, %obj, %state);
   }
};
activatePackage(StandinAmmo);

function TT_addAmmo(%obj, %type, %amount, %ignoreMax)
{
   %max = $Pref::Server::TT::Max[%type];
   if(%amount == -1)
      %amount = %max;
   %amount = mClamp(%amount, 0, %max - %obj.quantity[%type]);
   %obj.quantity[%type] += %amount;
   return %amount;
}

function TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread)
{
   for(%shell = 0; %shell < %shellCount; %shell++)
      %p = new Projectile() { dataBlock = %projectile; sourceObject = %obj; };
   return %p;
}

function TT_dampenVelocity(%obj, %divisor)
{
   %vel = %obj.getVelocity();
   %obj.setVelocity(vectorScale(%vel, 1 / %divisor));
}

function TT_processHeadshotDamage(%this, %obj, %col, %pos, %dmg, %multiplier, %headshotDmgType)
{
   %obj.sourceObject.client.play2D(bulletHitSound);
   %colscale = getWord(%col.getScale(), 2);
   if(getword(%pos, 2) > getword(%col.getWorldBoxCenter(), 2) - 3.3*%colscale)
   {
      %dmg *= %multiplier;
      %damageType = %headshotDmgType;
   }
   %col.damage(%obj, %pos, %dmg, %damageType);
}

// Every image's class answers no crit; an image with no test of its own
// runs this one through its className.
function WeaponImage::TT_isRaycastCritical(%this,%obj,%slot,%col,%pos,%normal,%hit)
{
	return 0;
}
