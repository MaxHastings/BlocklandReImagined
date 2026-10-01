// Stand-in melee weapons and saw, written for these tests.

datablock ItemData(L4BAxeItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Hatchet";
   image = L4BAxeImage;
   canDrop = true;
};

AddDamageType("L4BAxe", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(L4BAxeImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = L4BAxeItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 3;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinClinkSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 50;
   TT_raycastDirectDamageType = $DamageType::L4BAxe;

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

function L4BAxeImage::onFire(%this, %obj, %slot)
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

function L4BAxeImage::onActivate(%this, %obj, %slot)
{
   %obj.playthread(2, plant);
}

function L4BAxeImage::onPreFire(%this, %obj, %slot)
{
   %obj.playthread(2, shiftAway);
}

function L4BAxeImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
   return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}

datablock ItemData(L4BCookingKnifeItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Kitchen Knife";
   image = L4BCookingKnifeImage;
   canDrop = true;
};

AddDamageType("L4BCookingKnife", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(L4BCookingKnifeImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = L4BCookingKnifeItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 2.5;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinSliceSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 30;
   TT_raycastDirectDamageType = $DamageType::L4BCookingKnife;

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

function L4BCookingKnifeImage::onFire(%this, %obj, %slot)
{
   if(%obj.getDamagePercent() >= 1.0)
      return;
   %obj.playThread(2, shiftTo);
   if(getRandom(0,1))
   {
      %this.TT_raycastExplosionBrickSound = standinSliceSound;
   }
   else
   {
      %this.TT_raycastExplosionBrickSound = standinClinkSound;
   }
   Parent::onFire(%this, %obj, %slot);
}

function L4BCookingKnifeImage::onActivate(%this, %obj, %slot)
{
   %obj.playthread(2, plant);
}

function L4BCookingKnifeImage::onPreFire(%this, %obj, %slot)
{
   %obj.playthread(2, shiftAway);
}

function L4BCookingKnifeImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
   return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}

datablock ItemData(L4BKnifeonaStickItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Spear";
   image = L4BKnifeonaStickImage;
   canDrop = true;
};

AddDamageType("L4BKnifeonaStick", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(L4BKnifeonaStickImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = L4BKnifeonaStickItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 6;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinClinkSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 45;
   TT_raycastDirectDamageType = $DamageType::L4BKnifeonaStick;

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

function L4BKnifeonaStickImage::onFire(%this, %obj, %slot)
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

function L4BKnifeonaStickImage::onActivate(%this, %obj, %slot)
{
   %obj.playthread(2, plant);
}

function L4BKnifeonaStickImage::onPreFire(%this, %obj, %slot)
{
   %obj.playthread(2, shiftAway);
}

function L4BKnifeonaStickImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
   return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}

datablock ItemData(L4BNailbatItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Spiked Club";
   image = L4BNailbatImage;
   canDrop = true;
};

AddDamageType("L4BNailbat", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(L4BNailbatImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = L4BNailbatItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 3;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinThudSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 45;
   TT_raycastDirectDamageType = $DamageType::L4BNailbat;

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

function L4BNailbatImage::onFire(%this, %obj, %slot)
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
   WeaponImage::onFire(%this, %obj, %slot);
}

function L4BNailbatImage::onActivate(%this, %obj, %slot)
{
   %obj.playthread(2, plant);
}

function L4BNailbatImage::onPreFire(%this, %obj, %slot)
{
   %obj.playthread(2, shiftAway);
}

function L4BNailbatImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
   return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}

datablock ItemData(L4BPaddleItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Oar";
   image = L4BPaddleImage;
   canDrop = true;
};

AddDamageType("L4BPaddle", '%1 was struck down', '%2 struck down %1', 0.5, 1);
datablock ShapeBaseImageData(L4BPaddleImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = L4BPaddleItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   melee = true;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 3.5;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastExplosionBrickSound = standinThudSound;
   TT_raycastExplosionPlayerSound = standinThudSound;
   TT_raycastDirectDamage = 40;
   TT_raycastDirectDamageType = $DamageType::L4BPaddle;

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

function L4BPaddleImage::onFire(%this, %obj, %slot)
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
      %this.TT_raycastExplosionBrickSound = standinSliceSound;
   }
   Parent::onFire(%this, %obj, %slot);
}

function L4BPaddleImage::onActivate(%this, %obj, %slot)
{
   %obj.playthread(2, plant);
}

function L4BPaddleImage::onPreFire(%this, %obj, %slot)
{
   %obj.playthread(2, shiftAway);
}

function L4BPaddleImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
   return TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit);
}

datablock AudioProfile(standinSawSound)
{
   filename = "./saw.wav";
   description = AudioClosest3d;
   preload = true;
};

datablock ItemData(L4BChainsawItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./standin.dts";
   uiName = "Power Saw";
   image = L4BChainsawImage;
   canDrop = true;
};

AddDamageType("L4BChainsaw", '%1 was cut down', '%2 cut down %1', 0.5, 1);
datablock ShapeBaseImageData(L4BChainsawImage)
{
   shapeFile = "./standin.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = L4BChainsawItem;
   projectile = hammerProjectile;
   projectileType = Projectile;
   armReady = true;
   TT_raycastEnabled = true;
   TT_raycastWeaponRange = 4;
   TT_raycastWeaponTargets = $TypeMasks::FxBrickObjectType | $TypeMasks::PlayerObjectType;
   TT_raycastDirectDamage = 5;
   TT_raycastDirectDamageType = $DamageType::L4BChainsaw;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.2;
   stateTransitionOnTimeout[0] = "Ready";

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "Fire";

   stateName[2] = "Fire";
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "Fire";
   stateTransitionOnTriggerUp[2] = "Slow";
   stateFire[2] = true;
   stateScript[2] = "onFire";
   stateSound[2] = standinSawSound;

   stateName[3] = "Slow";
   stateTimeoutValue[3] = 0.25;
   stateTransitionOnTimeout[3] = "Ready";
   stateTransitionOnTriggerDown[3] = "Fire";
   stateWaitForTimeout[3] = true;
};

function L4BChainsawImage::onFire(%this, %obj, %slot)
{
   %obj.playThread(2, plant);
   Parent::onFire(%this, %obj, %slot);
}

function L4BChainsawImage::onMount(%this, %obj, %slot)
{
   %obj.playThread(0, plant);
   Parent::onMount(%this, %obj, %slot);
}
