// Stand-in for the Weapon_Package_Tier2 port tests (CC0): our own guns on
// the stand-in Tier 1, under the datablock and method names the port covers.
ForceRequiredAddOn("Weapon_Package_Tier1");

AddDamageType("StandinBattle", '%1 shot themselves', '%2 shot %1', 0.75, 1);
AddDamageType("StandinMagnum", '%1 shot themselves', '%2 shot %1', 0.75, 1);
AddDamageType("MagnumHeadshot", '%1 shot themselves', '%2 headshot %1', 0.75, 1);
AddDamageType("StandinMarksman", '%1 shot themselves', '%2 sniped %1', 0.75, 1);
AddDamageType("MarksmanHeadshot", '%1 shot themselves', '%2 critted %1', 0.75, 1);

// A body laid over the player and lifted again, the shape kept.
function Player::pushDatablock(%this,%data)
{
	if(fileName(%this.dataBlock.shapeFile) !$= fileName(%data.shapeFile))
	return;
}

function Player::popDatablock(%this,%data)
{
	%this.setDatablock(%this.altData[%this.altDataNum - 1]);
}

// A jet press reaches the held image.
package StandinAltTrigger
{
   function Armor::onTrigger(%db, %player, %triggerSlot, %val)
   {
	if(isObject(%image = %player.getMountedImage(%i)) && %image.altTriggerEnabled)
	%image.onAltTrigger(%player, %db, %triggerSlot, %val);
}
};
activatePackage(StandinAltTrigger);

// The carbine: its first round after a pause is a truer one.
datablock ProjectileData(TAssaultRifleProjectile1 : standinSMGProjectile)
{
   directDamage = 6;
};

datablock ProjectileData(TAssaultRifleProjectile2 : TAssaultRifleProjectile1)
{
   directDamage = 9;
};

datablock ItemData(TAssaultRifleItem : standinSidearmItem)
{
   uiName = "Stand-in Carbine";
   image = TAssaultRifleImage;
   TT_ammoType = "556";
   TT_maxAmmo = 8;
};

datablock ShapeBaseImageData(TAssaultRifleImage : standinSidearmImage)
{
   item = TAssaultRifleItem;
   projectile = TAssaultRifleProjectile1;
   TT_raycastEnabled = false;
};

function TAssaultRifleImage::onFire(%this,%obj,%slot)
{
	if((getSimTime() - %obj.lastShotTime) > 400)
	{
	%projectile = TAssaultRifleProjectile2;
	%spread = 0.0002;
	}
	else
	{
	%projectile = %this.projectile;
	%spread = 0.003;
	}
	%shellCount = 1;
	%obj.lastShotTime = getSimTime();
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

// The battle rifle: a gunner pushed up or down, a round that slows.
datablock ProjectileData(BattleRifleProjectile1 : standinRifleProjectile)
{
   directDamage = 14;
   directDamageType = $DamageType::StandinBattle;
};

datablock ItemData(BattleRifleItem : standinSidearmItem)
{
   uiName = "Stand-in Battle";
   image = BattleRifleImage;
   TT_ammoType = "556";
   TT_maxAmmo = 5;
};

datablock ShapeBaseImageData(BattleRifleImage : standinSidearmImage)
{
   item = BattleRifleItem;
   projectile = BattleRifleProjectile1;
   TT_raycastEnabled = false;
};

function BattleRifleImage::onFire(%this,%obj,%slot)
{
	TT_knockback(%obj, 0, 0, -2);
	%spread = 0.0005;
	%shellCount = 1;
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function BattleRifleProjectile1::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	if(%col.getType() & $TypeMasks::PlayerObjectType)
	TT_dampenVelocity(%col, 3);
}

// The machine gun: a second, free and tighter round each cycle, and a
// slower body while it fires.
datablock PlayerData(LMGArmor : PlayerStandardArmor)
{
   maxForwardSpeed = 3;
   maxBackwardSpeed = 3;
   maxSideSpeed = 3;
   canJet = false;
   uiName = "";
   firstPersonOnly = 1;
   isSurvivor = 1;
};

datablock ProjectileData(LightMachinegunProjectile1 : standinSMGProjectile)
{
   directDamage = 5;
};

datablock ItemData(LightMachinegunItem : standinSidearmItem)
{
   uiName = "Stand-in Gunner";
   image = LightMachinegunImage;
   TT_ammoType = "556";
   TT_maxAmmo = 12;
};

datablock ShapeBaseImageData(LightMachinegunImage : standinSidearmImage)
{
   item = LightMachinegunItem;
   projectile = LightMachinegunProjectile1;
   TT_raycastEnabled = false;

   stateTransitionOnTriggerDown[1]  = "Click";
   stateTransitionOnNotLoaded[3]    = "EmptyTransition";
   stateTimeoutValue[4]             = 0.02;
   stateTransitionOnTimeout[4]      = "Fire2";
   stateTransitionOnTriggerUp[5]    = "Halt";
   stateTimeoutValue[5]             = 0.02;
   stateTransitionOnTimeout[5]      = "FireCheckA";

   stateName[14]                    = "Click";
   stateScript[14]                  = "onClick";
   stateTimeoutValue[14]            = 0.01;
   stateTransitionOnTimeout[14]     = "FireCheckA";

   stateName[15]                    = "Fire2";
   stateScript[15]                  = "onFire2";
   stateTimeoutValue[15]            = 0.02;
   stateTransitionOnTimeout[15]     = "Smoke";
   stateAllowImageChange[15]        = false;

   stateName[16]                    = "Halt";
   stateScript[16]                  = "onHalt";
   stateTimeoutValue[16]            = 0.01;
   stateTransitionOnTimeout[16]     = "LoadCheckA";

   stateName[17]                    = "EmptyTransition";
   stateScript[17]                  = "onEmptyTransition";
   stateTimeoutValue[17]            = 0.01;
   stateTransitionOnTimeout[17]     = "EmptyFire";
};

function LightMachinegunImage::onFire(%this,%obj,%slot)
{
	TT_knockback(%obj, 0, 0, -1);
	%spread = 0.002;
	%shellCount = 1;
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function LightMachinegunImage::onFire2(%this,%obj,%slot)
{
	TT_knockback(%obj, 0, 0, -1);
	%shellCount = 1;
	%spread = 0.0005;
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function LightMachinegunImage::onClick(%this,%obj,%slot)
{
	%obj.pushDatablock(LMGArmor.getID());
}

function LightMachinegunImage::onHalt(%this,%obj,%slot)
{
	%obj.popDatablock(LMGArmor.getID());
}

function LightMachinegunImage::onEmptyTransition(%this,%obj,%slot)
{
	%obj.popDatablock(LMGArmor.getID());
}

function LightMachinegunImage::onUnMount(%this,%obj,%slot)
{
	%obj.popDatablock(LMGArmor.getID());
}

function LightMachinegunProjectile1::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	if(%col.getType() & $TypeMasks::PlayerObjectType)
	TT_dampenVelocity(%col, 2);
}

// The combat shotgun, whose jet press mounts its double blast.
datablock ProjectileData(CombatShotgunBlastProjectile : standinBlastProjectile)
{
   directDamage = 2;
};

datablock ItemData(CombatShotgunItem : standinPumpItem)
{
   uiName = "Stand-in Combat";
   image = CombatShotgunImage;
   TT_maxAmmo = 4;
};

datablock ShapeBaseImageData(CombatShotgunImage : standinPumpImage)
{
   item = CombatShotgunItem;
   altTriggerEnabled = true;
};

function CombatShotgunImage::onFire(%this,%obj,%slot)
{
	%this.TT_decrementAmmo(%obj);
	%projectile = %this.projectile;
	%spread = 0.004;
	%shellCount = 5;
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function CombatShotgunImage::onAltTrigger(%this,%obj,%objDB,%triggerSlot,%val)
{
	%obj.mountImage(combatShotgunAltfireImage, 0);
}

datablock ShapeBaseImageData(CombatShotgunAltfireImage)
{
   shapeFile = "./pump.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = CombatShotgunItem;
   projectile = standinPelletProjectile;
   projectileType = Projectile;
   armReady = true;

   stateName[0]                     = "Activate";
   stateTimeoutValue[0]             = 0.1;
   stateTransitionOnTimeout[0]      = "Fire";

   stateName[1]                     = "Fire";
   stateFire[1]                     = true;
   stateScript[1]                   = "onFire";
   stateTimeoutValue[1]             = 0.2;
   stateTransitionOnTimeout[1]      = "Done";
   stateAllowImageChange[1]         = false;
   stateWaitForTimeout[1]           = true;

   stateName[2]                     = "Done";
   stateScript[2]                   = "onDone";
};

function CombatShotgunAltfireImage::onFire(%this,%obj,%slot)
{
	if(%this.TT_canFire(%obj, 2))
	{
	%this.TT_decrementAmmo(%obj, 2);
	%projectile = %this.projectile;
	%spread = 0.004;
	%shellcount = 10;
	TT_createProjectile(%this, %obj, %slot, %projectile, %shellcount, %spread);
	%projectile = CombatShotgunBlastProjectile;
	%spread = 0.001;
	%shellcount = 3;
	TT_createProjectile(%this, %obj, %slot, %projectile, %shellcount, %spread);
	}
}

function CombatShotgunAltfireImage::onDone(%this,%obj,%slot)
{
	%obj.mountImage(CombatShotgunImage, %slot);
}

// The magnum, with headshots, and its scoped twin, an easter egg.
datablock ProjectileData(MagnumProjectile : standinRifleProjectile)
{
   directDamage = 25;
   directDamageType = $DamageType::StandinMagnum;
};

datablock ItemData(MagnumItem : standinSidearmItem)
{
   uiName = "Stand-in Magnum";
   image = MagnumImage;
   TT_maxAmmo = 4;
};

datablock ShapeBaseImageData(MagnumImage : standinSidearmImage)
{
   item = MagnumItem;
   projectile = MagnumProjectile;
   TT_raycastEnabled = false;
};

function MagnumImage::onFire(%this,%obj,%slot)
{
	%spread = 0.0001;
	%shellCount = 1;
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function MagnumProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	TT_processHeadshotDamage(%this, %obj, %col, %pos, %this.directDamage, 2, $DamageType::MagnumHeadshot);
}

datablock ItemData(ScopedMagnumItem : MagnumItem)
{
   uiName = "Stand-in Scoped";
   image = ScopedMagnumImage;
};

datablock ShapeBaseImageData(ScopedMagnumImage : MagnumImage)
{
   item = ScopedMagnumItem;
};

// The marksman rifle: a ray whose head hits crit.
datablock ItemData(MilitarySniperItem : standinSidearmItem)
{
   uiName = "Stand-in Marksman";
   image = MilitarySniperImage;
   TT_ammoType = "556";
   TT_maxAmmo = 3;
};

datablock ShapeBaseImageData(MilitarySniperImage : standinSidearmImage)
{
   item = MilitarySniperItem;
   TT_raycastWeaponRange = 300;
   TT_raycastDirectDamage = 20;
   TT_raycastDirectDamageType = $DamageType::StandinMarksman;
   TT_raycastCritDirectDamageType = $DamageType::MarksmanHeadshot;
   TT_raycastSpreadAmt = 0;
   TT_raycastFromMuzzle = false;
};

function MilitarySniperImage::onFire(%this,%obj,%slot)
{
   %this.TT_decrementAmmo(%obj);
   return Parent::onFire(%this,%obj,%slot);
}

function MilitarySniperImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isRaycastHeadshot(%this, %obj, %slot, %col, %pos, %normal, %hit);
}
