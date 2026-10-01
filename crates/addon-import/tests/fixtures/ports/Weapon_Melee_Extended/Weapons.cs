// Stand-in melee weapons, written for these tests.

datablock ItemData(CombatKnifeItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Field Knife";
   image = CombatKnifeImage;
   canDrop = true;
};

AddDamageType("CombatKnife", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(CombatKnifeImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = CombatKnifeItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 3;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinClinkSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 40;
   TT_raycastDirectDamageType = $DamageType::CombatKnife;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.25;
   stateTransitionOnTimeout[0] = "Ready";
   stateSound[0] = weaponSwitchSound;

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "Charge";

   stateName[2] = "Charge";
   stateTimeoutValue[2] = 0.5;
   stateTransitionOnTimeout[2] = "Armed";
   stateTransitionOnTriggerUp[2] = "Stab";
   stateWaitForTimeout[2] = false;
   stateScript[2] = "onCharge";

   stateName[3] = "Stab";
   stateTimeoutValue[3] = 0.2;
   stateTransitionOnTimeout[3] = "Ready";
   stateScript[3] = "onStabFire";
   stateWaitForTimeout[3] = true;

   stateName[4] = "Armed";
   stateTransitionOnTriggerUp[4] = "Slash";
   stateScript[4] = "onCharge";

   stateName[5] = "Slash";
   stateTimeoutValue[5] = 0.3;
   stateTransitionOnTimeout[5] = "Ready";
   stateFire[5] = true;
   stateScript[5] = "onFire";
   stateWaitForTimeout[5] = true;
};

function CombatKnifeImage::onFire(%this, %obj, %slot)
{
	%obj.playThread(2, shiftTo);
	if(%obj.getDamagePercent() >= 1.0)
	{
	%slot = 0;
	}
	else
	{
	%slot = 3;
	}
	%obj.playThread(%slot, spearThrow);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinSliceSound;
	}
	%this.TT_raycastDirectDamage = 120;
	Parent::onFire(%this, %obj, %slot);
}

function CombatKnifeImage::onStabFire(%this, %obj, %slot)
{
	%obj.playThread(2, shiftTo);
	if(%obj.getDamagePercent() >= 1.0)
	{
	%slot = 0;
	}
	else
	{
	%slot = 3;
	}
	%obj.playThread(%slot, shiftDown);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinSliceSound;
	}
	%this.TT_raycastDirectDamage = 40;
	Parent::onFire(%this, %obj, %slot);
}

function CombatKnifeImage::onCharge(%this, %obj, %slot)
{
   %obj.playthread(2, plant);
}

function CombatKnifeImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}

datablock ItemData(iceaxeItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Ice Pick";
   image = iceaxeImage;
   canDrop = true;
};

AddDamageType("iceaxe", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(iceaxeImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = iceaxeItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 4;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinClinkSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 35;
   TT_raycastDirectDamageType = $DamageType::iceaxe;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.25;
   stateTransitionOnTimeout[0] = "Ready";
   stateScript[0] = "onActivate";
   stateSound[0] = weaponSwitchSound;

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";

   stateName[2] = "PreFire";
   stateScript[2] = "onPreFire";
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "FireA";

   stateName[3] = "FireA";
   stateTimeoutValue[3] = 0.1;
   stateTransitionOnTimeout[3] = "FireB";
   stateSound[3] = TT_MeleeSwingSound;
   stateWaitForTimeout[3] = true;

   stateName[4] = "FireB";
   stateTimeoutValue[4] = 0.1;
   stateTransitionOnTimeout[4] = "Wait";
   stateFire[4] = true;
   stateScript[4] = "onFire";
   stateWaitForTimeout[4] = true;

   stateName[5] = "Wait";
   stateTimeoutValue[5] = 0.2;
   stateTransitionOnTimeout[5] = "CheckFire";
   stateWaitForTimeout[5] = true;

   stateName[6] = "CheckFire";
   stateTransitionOnTriggerUp[6] = "Ready";
   stateTransitionOnTriggerDown[6] = "PreFire";
};

function iceaxeImage::onFire(%this, %obj, %slot)
{
	if(%obj.getDamagePercent() >= 1.0)
	return;
	%obj.playThread(2, shiftTo);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinSliceSound;
	}
	Parent::onFire(%this, %obj, %slot);
}

function iceaxeImage::onActivate(%this, %obj, %slot)
{
	%obj.playthread(2, plant);
}

function iceaxeImage::onPreFire(%this, %obj, %slot)
{
	%obj.playthread(2, shiftAway);
}

function iceaxeImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}


datablock ItemData(L4BBatonItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Nightstick";
   image = L4BBatonImage;
   canDrop = true;
};

AddDamageType("L4BBaton", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(L4BBatonImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = L4BBatonItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 4;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinClinkSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 30;
   TT_raycastDirectDamageType = $DamageType::L4BBaton;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.25;
   stateTransitionOnTimeout[0] = "Ready";
   stateScript[0] = "onActivate";
   stateSound[0] = weaponSwitchSound;

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";

   stateName[2] = "PreFire";
   stateScript[2] = "onPreFire";
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "FireA";

   stateName[3] = "FireA";
   stateTimeoutValue[3] = 0.1;
   stateTransitionOnTimeout[3] = "FireB";
   stateSound[3] = TT_MeleeSwingSound;
   stateWaitForTimeout[3] = true;

   stateName[4] = "FireB";
   stateTimeoutValue[4] = 0.1;
   stateTransitionOnTimeout[4] = "Wait";
   stateFire[4] = true;
   stateScript[4] = "onFire";
   stateWaitForTimeout[4] = true;

   stateName[5] = "Wait";
   stateTimeoutValue[5] = 0.2;
   stateTransitionOnTimeout[5] = "CheckFire";
   stateWaitForTimeout[5] = true;

   stateName[6] = "CheckFire";
   stateTransitionOnTriggerUp[6] = "Ready";
   stateTransitionOnTriggerDown[6] = "PreFire";
};

function L4BBatonImage::onFire(%this, %obj, %slot)
{
	if(%obj.getDamagePercent() >= 1.0)
	return;
	%obj.playThread(2, shiftTo);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinSliceSound;
	}
	Parent::onFire(%this, %obj, %slot);
}

function L4BBatonImage::onActivate(%this, %obj, %slot)
{
	%obj.playthread(2, plant);
}

function L4BBatonImage::onPreFire(%this, %obj, %slot)
{
	%obj.playthread(2, shiftAway);
}

function L4BBatonImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}


datablock ItemData(L4BCrowbarItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Pry Bar";
   image = L4BCrowbarImage;
   canDrop = true;
};

AddDamageType("L4BCrowbar", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(L4BCrowbarImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = L4BCrowbarItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 4;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinClinkSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 30;
   TT_raycastDirectDamageType = $DamageType::L4BCrowbar;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.25;
   stateTransitionOnTimeout[0] = "Ready";
   stateScript[0] = "onActivate";
   stateSound[0] = weaponSwitchSound;

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";

   stateName[2] = "PreFire";
   stateScript[2] = "onPreFire";
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "FireA";

   stateName[3] = "FireA";
   stateTimeoutValue[3] = 0.1;
   stateTransitionOnTimeout[3] = "FireB";
   stateSound[3] = TT_MeleeSwingSound;
   stateWaitForTimeout[3] = true;

   stateName[4] = "FireB";
   stateTimeoutValue[4] = 0.1;
   stateTransitionOnTimeout[4] = "Wait";
   stateFire[4] = true;
   stateScript[4] = "onFire";
   stateWaitForTimeout[4] = true;

   stateName[5] = "Wait";
   stateTimeoutValue[5] = 0.2;
   stateTransitionOnTimeout[5] = "CheckFire";
   stateWaitForTimeout[5] = true;

   stateName[6] = "CheckFire";
   stateTransitionOnTriggerUp[6] = "Ready";
   stateTransitionOnTriggerDown[6] = "PreFire";
};

function L4BCrowbarImage::onFire(%this, %obj, %slot)
{
	if(%obj.getDamagePercent() >= 1.0)
	return;
	%obj.playThread(2, shiftTo);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinSliceSound;
	}
	Parent::onFire(%this, %obj, %slot);
}

function L4BCrowbarImage::onActivate(%this, %obj, %slot)
{
	%obj.playthread(2, plant);
}

function L4BCrowbarImage::onPreFire(%this, %obj, %slot)
{
	%obj.playthread(2, shiftAway);
}

function L4BCrowbarImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}


datablock ItemData(l4bPanItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Skillet";
   image = l4bPanImage;
   canDrop = true;
};

AddDamageType("l4bPan", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(l4bPanImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = l4bPanItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 4;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinClinkSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 30;
   TT_raycastDirectDamageType = $DamageType::l4bPan;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.25;
   stateTransitionOnTimeout[0] = "Ready";
   stateScript[0] = "onActivate";
   stateSound[0] = weaponSwitchSound;

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";

   stateName[2] = "PreFire";
   stateScript[2] = "onPreFire";
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "FireA";

   stateName[3] = "FireA";
   stateTimeoutValue[3] = 0.1;
   stateTransitionOnTimeout[3] = "FireB";
   stateSound[3] = TT_MeleeSwingSound;
   stateWaitForTimeout[3] = true;

   stateName[4] = "FireB";
   stateTimeoutValue[4] = 0.1;
   stateTransitionOnTimeout[4] = "Wait";
   stateFire[4] = true;
   stateScript[4] = "onFire";
   stateWaitForTimeout[4] = true;

   stateName[5] = "Wait";
   stateTimeoutValue[5] = 0.2;
   stateTransitionOnTimeout[5] = "CheckFire";
   stateWaitForTimeout[5] = true;

   stateName[6] = "CheckFire";
   stateTransitionOnTriggerUp[6] = "Ready";
   stateTransitionOnTriggerDown[6] = "PreFire";
};

function l4bPanImage::onFire(%this, %obj, %slot)
{
	if(%obj.getDamagePercent() >= 1.0)
	return;
	%obj.playThread(2, shiftTo);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinSliceSound;
	}
	Parent::onFire(%this, %obj, %slot);
}

function l4bPanImage::onActivate(%this, %obj, %slot)
{
	%obj.playthread(2, plant);
}

function l4bPanImage::onPreFire(%this, %obj, %slot)
{
	%obj.playthread(2, shiftAway);
}

function l4bPanImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}


datablock ItemData(l4bGuitarItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Banjo";
   image = l4bGuitarImage;
   canDrop = true;
};

AddDamageType("l4bGuitar", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(l4bGuitarImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = l4bGuitarItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 4;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinClinkSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 30;
   TT_raycastDirectDamageType = $DamageType::l4bGuitar;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.25;
   stateTransitionOnTimeout[0] = "Ready";
   stateScript[0] = "onActivate";
   stateSound[0] = weaponSwitchSound;

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";

   stateName[2] = "PreFire";
   stateScript[2] = "onPreFire";
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "FireA";

   stateName[3] = "FireA";
   stateTimeoutValue[3] = 0.1;
   stateTransitionOnTimeout[3] = "FireB";
   stateSound[3] = TT_MeleeSwingSound;
   stateWaitForTimeout[3] = true;

   stateName[4] = "FireB";
   stateTimeoutValue[4] = 0.1;
   stateTransitionOnTimeout[4] = "Wait";
   stateFire[4] = true;
   stateScript[4] = "onFire";
   stateWaitForTimeout[4] = true;

   stateName[5] = "Wait";
   stateTimeoutValue[5] = 0.2;
   stateTransitionOnTimeout[5] = "CheckFire";
   stateWaitForTimeout[5] = true;

   stateName[6] = "CheckFire";
   stateTransitionOnTriggerUp[6] = "Ready";
   stateTransitionOnTriggerDown[6] = "PreFire";
};

function l4bGuitarImage::onFire(%this, %obj, %slot)
{
	if(%obj.getDamagePercent() >= 1.0)
	return;
	%obj.playThread(2, shiftTo);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	%this.TT_raycastExplosionPlayerSound = standinSliceSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinSliceSound;
	%this.TT_raycastExplosionPlayerSound = standinThudSound;
	}
	Parent::onFire(%this, %obj, %slot);
}

function l4bGuitarImage::onActivate(%this, %obj, %slot)
{
	%obj.playthread(2, activate);
}

function l4bGuitarImage::onPreFire(%this, %obj, %slot)
{
	%obj.playthread(2, shiftAway);
}

function l4bGuitarImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}


datablock ItemData(L4BKatanaItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Long Blade";
   image = L4BKatanaImage;
   canDrop = true;
};

AddDamageType("L4BKatana", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(L4BKatanaImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = L4BKatanaItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 4;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinClinkSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 45;
   TT_raycastDirectDamageType = $DamageType::L4BKatana;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.25;
   stateTransitionOnTimeout[0] = "Ready";
   stateScript[0] = "onActivate";
   stateSound[0] = weaponSwitchSound;

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";

   stateName[2] = "PreFire";
   stateScript[2] = "onPreFire";
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "FireA";

   stateName[3] = "FireA";
   stateTimeoutValue[3] = 0.1;
   stateTransitionOnTimeout[3] = "FireB";
   stateSound[3] = TT_MeleeSwingSound;
   stateWaitForTimeout[3] = true;

   stateName[4] = "FireB";
   stateTimeoutValue[4] = 0.1;
   stateTransitionOnTimeout[4] = "Wait";
   stateFire[4] = true;
   stateScript[4] = "onFire";
   stateWaitForTimeout[4] = true;

   stateName[5] = "Wait";
   stateTimeoutValue[5] = 0.2;
   stateTransitionOnTimeout[5] = "CheckFire";
   stateWaitForTimeout[5] = true;

   stateName[6] = "CheckFire";
   stateTransitionOnTriggerUp[6] = "Ready";
   stateTransitionOnTriggerDown[6] = "PreFire";
};

function L4BKatanaImage::onFire(%this, %obj, %slot)
{
	if(%obj.getDamagePercent() >= 1.0)
	return;
	%obj.playThread(2, shiftTo);
	%obj.playThread(3, shiftLeft);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinSliceSound;
	}
	Parent::onFire(%this, %obj, %slot);
}

function L4BKatanaImage::onActivate(%this, %obj, %slot)
{
	%obj.playthread(2, plant);
}

function L4BKatanaImage::onPreFire(%this, %obj, %slot)
{
	%obj.playthread(2, shiftAway);
}

function L4BKatanaImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}


datablock ItemData(l4bMacheteItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Cleaver";
   image = l4bMacheteImage;
   canDrop = true;
};

AddDamageType("l4bMachete", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(l4bMacheteImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = l4bMacheteItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 4;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinClinkSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 45;
   TT_raycastDirectDamageType = $DamageType::l4bMachete;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.25;
   stateTransitionOnTimeout[0] = "Ready";
   stateScript[0] = "onActivate";
   stateSound[0] = weaponSwitchSound;

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";

   stateName[2] = "PreFire";
   stateScript[2] = "onPreFire";
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "FireA";

   stateName[3] = "FireA";
   stateTimeoutValue[3] = 0.1;
   stateTransitionOnTimeout[3] = "FireB";
   stateSound[3] = TT_MeleeSwingSound;
   stateWaitForTimeout[3] = true;
   stateScript[3] = "onFireB";

   stateName[4] = "FireB";
   stateTimeoutValue[4] = 0.1;
   stateTransitionOnTimeout[4] = "Wait";
   stateFire[4] = true;
   stateScript[4] = "onFire";
   stateWaitForTimeout[4] = true;

   stateName[5] = "Wait";
   stateTimeoutValue[5] = 0.2;
   stateTransitionOnTimeout[5] = "CheckFire";
   stateWaitForTimeout[5] = true;

   stateName[6] = "CheckFire";
   stateTransitionOnTriggerUp[6] = "Ready";
   stateTransitionOnTriggerDown[6] = "PreFire";
};

function l4bMacheteImage::onFire(%this, %obj, %slot)
{
	if(%obj.getDamagePercent() >= 1.0)
	return;
	%obj.playThread(2, shiftTo);
	%obj.playThread(3, shiftLeft);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinSliceSound;
	}
	Parent::onFire(%this, %obj, %slot);
}

function l4bMacheteImage::onFireB(%this, %obj, %slot)
{
	%obj.playthread(2, shiftAway);
}

function l4bMacheteImage::onActivate(%this, %obj, %slot)
{
	%obj.playthread(2, shiftDown);
}

function l4bMacheteImage::onPreFire(%this, %obj, %slot)
{
	%obj.playthread(2, shiftAway);
}

function l4bMacheteImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}


datablock ItemData(L4BSpadeItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Trowel";
   image = L4BSpadeImage;
   canDrop = true;
};

AddDamageType("L4BSpade", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(L4BSpadeImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = L4BSpadeItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 4;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinClinkSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 30;
   TT_raycastDirectDamageType = $DamageType::L4BSpade;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.25;
   stateTransitionOnTimeout[0] = "Ready";
   stateScript[0] = "onActivate";
   stateSound[0] = weaponSwitchSound;

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";

   stateName[2] = "PreFire";
   stateScript[2] = "onPreFire";
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "FireA";

   stateName[3] = "FireA";
   stateTimeoutValue[3] = 0.1;
   stateTransitionOnTimeout[3] = "FireB";
   stateSound[3] = TT_MeleeSwingSound;
   stateWaitForTimeout[3] = true;

   stateName[4] = "FireB";
   stateTimeoutValue[4] = 0.1;
   stateTransitionOnTimeout[4] = "Wait";
   stateFire[4] = true;
   stateScript[4] = "onFire";
   stateWaitForTimeout[4] = true;

   stateName[5] = "Wait";
   stateTimeoutValue[5] = 0.2;
   stateTransitionOnTimeout[5] = "CheckFire";
   stateWaitForTimeout[5] = true;

   stateName[6] = "CheckFire";
   stateTransitionOnTriggerUp[6] = "Ready";
   stateTransitionOnTriggerDown[6] = "PreFire";
};

function L4BSpadeImage::onFire(%this, %obj, %slot)
{
	if(%obj.getDamagePercent() >= 1.0)
	return;
	%obj.playThread(2, shiftTo);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	%this.TT_raycastExplosionPlayerSound = standinSliceSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinSliceSound;
	%this.TT_raycastExplosionPlayerSound = standinThudSound;
	}
	Parent::onFire(%this, %obj, %slot);
}

function L4BSpadeImage::onActivate(%this, %obj, %slot)
{
	%obj.playthread(2, plant);
}

function L4BSpadeImage::onPreFire(%this, %obj, %slot)
{
	%obj.playthread(2, shiftAway);
}

function L4BSpadeImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}


datablock ItemData(pipeItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Rod";
   image = pipeImage;
   canDrop = true;
};

AddDamageType("pipe", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(pipeImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = pipeItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 4;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinClinkSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 30;
   TT_raycastDirectDamageType = $DamageType::pipe;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.25;
   stateTransitionOnTimeout[0] = "Ready";
   stateScript[0] = "onActivate";
   stateSound[0] = weaponSwitchSound;

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";

   stateName[2] = "PreFire";
   stateScript[2] = "onPreFire";
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "FireA";

   stateName[3] = "FireA";
   stateTimeoutValue[3] = 0.1;
   stateTransitionOnTimeout[3] = "FireB";
   stateSound[3] = TT_MeleeSwingSound;
   stateWaitForTimeout[3] = true;

   stateName[4] = "FireB";
   stateTimeoutValue[4] = 0.1;
   stateTransitionOnTimeout[4] = "Wait";
   stateFire[4] = true;
   stateScript[4] = "onFire";
   stateWaitForTimeout[4] = true;

   stateName[5] = "Wait";
   stateTimeoutValue[5] = 0.2;
   stateTransitionOnTimeout[5] = "CheckFire";
   stateWaitForTimeout[5] = true;

   stateName[6] = "CheckFire";
   stateTransitionOnTriggerUp[6] = "Ready";
   stateTransitionOnTriggerDown[6] = "PreFire";
};

function pipeImage::onFire(%this, %obj, %slot)
{
	if(%obj.getDamagePercent() >= 1.0)
	return;
	%obj.playThread(2, shiftTo);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	%this.TT_raycastExplosionPlayerSound = standinSliceSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinSliceSound;
	%this.TT_raycastExplosionPlayerSound = standinThudSound;
	}
	Parent::onFire(%this, %obj, %slot);
}

function pipeImage::onActivate(%this, %obj, %slot)
{
	%obj.playthread(2, shiftto);
}

function pipeImage::onPreFire(%this, %obj, %slot)
{
	%obj.playthread(2, shiftAway);
}

function pipeImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}


datablock ItemData(sledgehammerItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Mallet";
   image = sledgehammerImage;
   canDrop = true;
};

AddDamageType("sledgehammer", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(sledgehammerImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = sledgehammerItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 4;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinClinkSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 60;
   TT_raycastDirectDamageType = $DamageType::sledgehammer;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.25;
   stateTransitionOnTimeout[0] = "Ready";
   stateScript[0] = "onActivate";
   stateSound[0] = weaponSwitchSound;

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";

   stateName[2] = "PreFire";
   stateScript[2] = "onPreFire";
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "FireA";

   stateName[3] = "FireA";
   stateTimeoutValue[3] = 0.1;
   stateTransitionOnTimeout[3] = "FireB";
   stateSound[3] = TT_MeleeSwingSound;
   stateWaitForTimeout[3] = true;

   stateName[4] = "FireB";
   stateTimeoutValue[4] = 0.1;
   stateTransitionOnTimeout[4] = "Wait";
   stateFire[4] = true;
   stateScript[4] = "onFire";
   stateWaitForTimeout[4] = true;

   stateName[5] = "Wait";
   stateTimeoutValue[5] = 0.2;
   stateTransitionOnTimeout[5] = "CheckFire";
   stateWaitForTimeout[5] = true;

   stateName[6] = "CheckFire";
   stateTransitionOnTriggerUp[6] = "Ready";
   stateTransitionOnTriggerDown[6] = "PreFire";
};

function sledgehammerImage::onFire(%this, %obj, %slot)
{
	if(%obj.getDamagePercent() >= 1.0)
	return;
	%obj.playThread(2, shiftTo);
	if(getRandom(0,1))
	{
	%this.TT_raycastExplosionBrickSound = standinClinkSound;
	}
	else
	{
	%this.TT_raycastExplosionBrickSound = standinSliceSound;
	}
	Parent::onFire(%this, %obj, %slot);
}

function sledgehammerImage::onActivate(%this, %obj, %slot)
{
	%obj.playthread(2, plant);
}

function sledgehammerImage::onPreFire(%this, %obj, %slot)
{
	%obj.playthread(2, shiftAway);
}

function sledgehammerImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}

