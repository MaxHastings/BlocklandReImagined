// Stand-in hidden melee weapons, written for these tests.

datablock ItemData(L4BBigStickItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Long Stick";
   image = L4BBigStickImage;
   canDrop = true;
};

AddDamageType("L4BBigStick", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(L4BBigStickImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = L4BBigStickItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 3;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinThudSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 25;
   TT_raycastDirectDamageType = $DamageType::L4BBigStick;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.25;
   stateTransitionOnTimeout[0] = "Ready";
   stateScript[0] = "onActivate";

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";

   stateName[2] = "PreFire";
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "Fire";
   stateScript[2] = "onPreFire";

   stateName[3] = "Fire";
   stateTimeoutValue[3] = 0.4;
   stateTransitionOnTimeout[3] = "Ready";
   stateFire[3] = true;
   stateScript[3] = "onFire";
   stateWaitForTimeout[3] = true;
};

function L4BBigStickImage::onFire(%this, %obj, %slot)
{
	if(%obj.getDamagePercent() >= 1.0)
	return;
	%obj.playThread(2, shiftTo);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinThudSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	}
	Parent::onFire(%this, %obj, %slot);
}

function L4BBigStickImage::onActivate(%this, %obj, %slot)
{
	%obj.playthread(2, plant);
}

function L4BBigStickImage::onPreFire(%this, %obj, %slot)
{
	%obj.playthread(2, shiftAway);
}

function L4BBigStickImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}

datablock ItemData(L4BGaffItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Hook";
   image = L4BGaffImage;
   canDrop = true;
};

AddDamageType("L4BGaff", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(L4BGaffImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = L4BGaffItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 3.5;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinClinkSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 45;
   TT_raycastDirectDamageType = $DamageType::L4BGaff;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.25;
   stateTransitionOnTimeout[0] = "Ready";
   stateScript[0] = "onActivate";

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";

   stateName[2] = "PreFire";
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "Fire";
   stateScript[2] = "onPreFire";

   stateName[3] = "Fire";
   stateTimeoutValue[3] = 0.4;
   stateTransitionOnTimeout[3] = "Ready";
   stateFire[3] = true;
   stateScript[3] = "onFire";
   stateWaitForTimeout[3] = true;
};

function L4BGaffImage::onFire(%this, %obj, %slot)
{
	if(%obj.getDamagePercent() >= 1.0)
	return;
	%obj.playThread(2, shiftTo);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	%this.TT_raycastExplosionPlayerSound = standinClinkSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinSliceSound;
	%this.TT_raycastExplosionPlayerSound = standinSliceSound;
	}
	Parent::onFire(%this, %obj, %slot);
}

function L4BGaffImage::onActivate(%this, %obj, %slot)
{
	%obj.playthread(2, plant);
}

function L4BGaffImage::onPreFire(%this, %obj, %slot)
{
	%obj.playthread(2, shiftAway);
}

function L4BGaffImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}

datablock ItemData(L4BPipeWrenchItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Wrench";
   image = L4BPipeWrenchImage;
   canDrop = true;
};

AddDamageType("L4BPipeWrench", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(L4BPipeWrenchImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = L4BPipeWrenchItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 3;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinThudSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 40;
   TT_raycastDirectDamageType = $DamageType::L4BPipeWrench;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.25;
   stateTransitionOnTimeout[0] = "Ready";
   stateScript[0] = "onActivate";

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";

   stateName[2] = "PreFire";
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "Fire";
   stateScript[2] = "onPreFire";

   stateName[3] = "Fire";
   stateTimeoutValue[3] = 0.4;
   stateTransitionOnTimeout[3] = "Ready";
   stateFire[3] = true;
   stateScript[3] = "onFire";
   stateWaitForTimeout[3] = true;
};

function L4BPipeWrenchImage::onFire(%this, %obj, %slot)
{
	if(%obj.getDamagePercent() >= 1.0)
	return;
	%obj.playThread(2, shiftTo);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinThudSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	}
	Parent::onFire(%this, %obj, %slot);
}

function L4BPipeWrenchImage::onActivate(%this, %obj, %slot)
{
	%obj.playthread(2, shiftto);
}

function L4BPipeWrenchImage::onPreFire(%this, %obj, %slot)
{
	%obj.playthread(2, shiftAway);
}

function L4BPipeWrenchImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}

