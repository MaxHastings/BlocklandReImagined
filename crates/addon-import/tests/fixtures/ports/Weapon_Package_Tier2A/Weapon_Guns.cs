AddDamageType("StandinBurst", '%1 shot themselves', '%2 shot %1', 0.75, 1);
AddDamageType("StandinTwins", '%1 shot themselves', '%2 shot %1', 0.75, 1);
AddDamageType("StandinScope", '%1 shot themselves', '%2 scoped %1', 0.75, 1);
AddDamageType("StandinMachine", '%1 shot themselves', '%2 shot %1', 0.75, 1);
AddDamageType("StandinMatch", '%1 shot themselves', '%2 shot %1', 0.75, 1);

// The burst rifle: each pull fires three, the burst checks taking their
// own rounds.
datablock ProjectileData(BullpupProjectile1 : standinSMGProjectile)
{
   directDamage = 5;
   directDamageType = $DamageType::StandinBurst;
};

datablock ItemData(BullpupItem : standinSidearmItem)
{
   uiName = "Stand-in Burst";
   image = BullpupImage;
   TT_ammoType = "556";
   TT_maxAmmo = 7;
};

datablock ShapeBaseImageData(BullpupImage : standinSidearmImage)
{
   item = BullpupItem;
   projectile = BullpupProjectile1;
   TT_raycastEnabled = false;

   stateTransitionOnTimeout[4]      = "BurstCheck1A";

   stateName[14]                    = "BurstCheck1A";
   stateScript[14]                  = "onBurstCheck";
   stateTimeoutValue[14]            = 0.03;
   stateTransitionOnTimeout[14]     = "BurstCheck1B";

   stateName[15]                    = "BurstCheck1B";
   stateTransitionOnLoaded[15]      = "Fire1";
   stateTransitionOnNotLoaded[15]   = "Smoke";

   stateName[16]                    = "Fire1";
   stateFire[16]                    = true;
   stateScript[16]                  = "onBurstFire";
   stateTimeoutValue[16]            = 0.05;
   stateTransitionOnTimeout[16]     = "BurstCheck2A";

   stateName[17]                    = "BurstCheck2A";
   stateScript[17]                  = "onBurstCheck";
   stateTimeoutValue[17]            = 0.03;
   stateTransitionOnTimeout[17]     = "BurstCheck2B";

   stateName[18]                    = "BurstCheck2B";
   stateTransitionOnLoaded[18]      = "Fire2";
   stateTransitionOnNotLoaded[18]   = "Smoke";

   stateName[19]                    = "Fire2";
   stateFire[19]                    = true;
   stateScript[19]                  = "onBurstFire";
   stateTimeoutValue[19]            = 0.05;
   stateTransitionOnTimeout[19]     = "Smoke";
};

function BullpupImage::onFire(%this,%obj,%slot)
{
	if(vectorLen(%obj.getVelocity()) < 0.1)
	{
	%spread = 0.0002;
	}
	else
	{
	%spread = 0.0008;
	%shellCount = 1;
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
	}
}

function BullpupImage::onBurstFire(%this,%obj,%slot)
{
	if(vectorLen(%obj.getVelocity()) < 0.1)
	{
	%spread = 0.0002;
	}
	else
	{
	%spread = 0.0008;
	%shellCount = 1;
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
	}
}

function BullpupImage::onBurstCheck(%this, %obj, %slot)
{
	if(%this.TT_canFire(%obj))
	{
	%obj.setImageLoaded(%slot, 1);
	%this.TT_decrementAmmo(%obj);
	}
}

// The twin guns: the right one pays, the left one fires beside it free.
datablock ProjectileData(DualSMGsProjectile1 : standinSMGProjectile)
{
   directDamage = 4;
   directDamageType = $DamageType::StandinTwins;
};

datablock ItemData(DualSMGsItem : standinSidearmItem)
{
   uiName = "Stand-in Twins";
   image = DualSMGsImage;
   TT_maxAmmo = 10;
};

datablock ShapeBaseImageData(DualSMGsImage : standinPairImage)
{
   item = DualSMGsItem;
   projectile = DualSMGsProjectile1;
   TT_raycastEnabled = false;
};

datablock ShapeBaseImageData(DualSMGLeftImage : standinLeftImage)
{
   item = DualSMGsItem;
   projectile = DualSMGsProjectile1;
   TT_raycastEnabled = false;
};

function DualSMGsImage::onMount(%this,%obj,%slot)
{
	%obj.mountImage(DualSMGLeftImage, 1);
}

function DualSMGLeftImage::onMount(%this,%obj,%slot)
{
	%obj.playThread(1, armreadyboth);
}

function DualSMGsImage::onFireAkimbo(%this,%obj,%slot)
{
	%obj.setImageTrigger(1,1);
}

function DualSMGsImage::onFire(%this,%obj,%slot)
{
	%spread = 0.0015;
	%shellCount = 1;
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function DualSMGLeftImage::onFire(%this, %obj, %slot)
{
	%spread = 0.0015;
	%shellCount = 1;
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function DualSMGsProjectile1::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	if(%col.getType() & $TypeMasks::PlayerObjectType)
	TT_dampenVelocity(%col, 1.1);
}

// The machine pistol, less sure on the move.
datablock ItemData(MachstilItem : standinSidearmItem)
{
   uiName = "Stand-in Machine Pistol";
   image = MachStilImage;
   TT_maxAmmo = 12;
};

datablock ShapeBaseImageData(MachStilImage : standinSidearmImage)
{
   item = MachstilItem;
   TT_raycastDirectDamage = 6;
   TT_raycastDirectDamageType = $DamageType::StandinMachine;
};

function MachStilImage::onFire(%this,%obj,%slot)
{
	if(vectorLen(%obj.getVelocity()) > 0.1)
	{
	%this.TT_raycastSpreadAmt = 0.0024;
	%this.TT_raycastWeaponRange = 45;
	}
	else
	{
	%this.TT_raycastSpreadAmt = 0.0013;
	%this.TT_raycastWeaponRange = 100;
	}
}

// The scoped rifle: jet scopes in, slowed by Tier 2's laid body, and out
// again; a reload begun scoped hands back to the unscoped rifle.
datablock ItemData(SniperCarbineItem : standinSidearmItem)
{
   uiName = "Stand-in Scope";
   image = SniperCarbineImage;
   TT_ammoType = "556";
   TT_maxAmmo = 5;
};

datablock ShapeBaseImageData(SniperCarbineImage : standinSidearmImage)
{
   item = SniperCarbineItem;
   altTriggerEnabled = true;
   TT_raycastWeaponRange = 70;
   TT_raycastDirectDamage = 12;
   TT_raycastDirectDamageType = $DamageType::StandinScope;
};

datablock ShapeBaseImageData(SniperCZoomedImage : SniperCarbineImage)
{
   TT_raycastDirectDamage = 14;
   TT_raycastSpreadAmt = 0.0001;

   stateTransitionOnNotLoaded[1]    = "ScopeOut";
   stateTransitionOnNotLoaded[7]    = "ScopeOut";

   stateName[14]                    = "ScopeOut";
   stateScript[14]                  = "onReloadStart";
};

function SniperCarbineImage::onFire(%this,%obj,%slot)
{
   Parent::onFire(%this,%obj,%slot);
   %this.TT_decrementAmmo(%obj);
}

function SniperCarbineImage::onAltTrigger(%this, %obj, %objDB, %triggerSlot, %val)
{
	%obj.mountImage(SniperCZoomedImage, 0);
}

function SniperCarbineImage::TT_onLoadCheck(%this,%obj,%slot)
{
	if(%obj.TT_forceToolReload || %this.TT_needsAmmo(%obj))
	%obj.setImageLoaded(%slot, 0);
}

function SniperCZoomedImage::onFire(%this,%obj,%slot)
{
   Parent::onFire(%this,%obj,%slot);
   %this.TT_decrementAmmo(%obj);
}

function SniperCZoomedImage::onMount(%this,%obj,%slot)
{
	%obj.pushDatablock(LMGArmor.getID());
}

function SniperCZoomedImage::onUnMount(%this,%obj,%slot)
{
	%obj.popDatablock(LMGArmor.getID());
}

function SniperCZoomedImage::onReloadStart(%this,%obj,%slot)
{
	%obj.TT_forceToolReload = 1;
	%obj.mountImage(SniperCarbineImage, 0);
}

function SniperCZoomedImage::onAltTrigger(%this, %obj, %objDB, %triggerSlot, %val)
{
	%obj.mountImage(SniperCarbineImage, 0);
}

function SniperCZoomedImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isRaycastHeadshot(%this, %obj, %slot, %col, %pos, %normal, %hit);
}
