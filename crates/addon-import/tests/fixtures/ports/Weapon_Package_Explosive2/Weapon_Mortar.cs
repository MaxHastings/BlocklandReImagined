// The mortar, one of the hidden extras: lobbed to come down about where
// the holder looks.
AddDamageType("MortarDirect", '%1 was shelled', '%2 shelled %1', 1, 1);
datablock ProjectileData(MortarProjectile)
{
   projectileShapeName = "./shell.dts";
   directDamage = 10;
   directDamageType = $DamageType::MortarDirect;
   radiusDamageType = $DamageType::MortarDirect;
   explosion = standinRocketExplosion;
   muzzleVelocity = 50;
   velInheritFactor = 0;
   armingDelay = 1000;
   lifetime = 8000;
   fadeDelay = 8000;
   bounceElasticity = 0.2;
   bounceFriction = 1;
   isBallistic = true;
   explodeOnPlayerImpact = true;
   explodeOnDeath = true;
};

datablock ItemData(MortarItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./launcher.dts";
   uiName = "Stand-in Mortar";
   image = MortarImage;
   canDrop = true;
   TT_ammoType = "Bomb";
   TT_reloads = true;
   TT_maxAmmo = 1;
   TT_alwaysReloadPref = "Ex";
};

datablock ShapeBaseImageData(MortarImage)
{
   shapeFile = "./launcher.dts";
   mountPoint = 0;
   correctMuzzleVector = true;
   className = "WeaponImage";
   item = MortarItem;
   projectile = MortarProjectile;
   projectileType = Projectile;
   armReady = true;

   stateName[0]                     = "Activate";
   stateTimeoutValue[0]             = 0.3;
   stateTransitionOnTimeout[0]      = "LoadCheckA";

   stateName[1]                     = "Ready";
   stateTransitionOnTriggerDown[1]  = "FireCheckA";
   stateTransitionOnNotLoaded[1]    = "ReloadStart";

   stateName[2]                     = "Fire";
   stateTransitionOnTimeout[2]      = "Delay";
   stateTimeoutValue[2]             = 0.01;
   stateFire[2]                     = true;
   stateAllowImageChange[2]         = false;
   stateScript[2]                   = "onFire";
   stateSound[2]                    = standinLaunchSound;

   stateName[3]                     = "Delay";
   stateTransitionOnTimeout[3]      = "ReloadStart";
   stateTimeoutValue[3]             = 0.1;

   stateName[4]                     = "LoadCheckA";
   stateScript[4]                   = "TT_onLoadCheck";
   stateTimeoutValue[4]             = 0.01;
   stateTransitionOnTimeout[4]      = "LoadCheckB";

   stateName[5]                     = "LoadCheckB";
   stateTransitionOnLoaded[5]       = "Ready";
   stateTransitionOnNotLoaded[5]    = "Empty";

   stateName[6]                     = "Reload";
   stateTimeoutValue[6]             = 1.0;
   stateScript[6]                   = "onReloadStart";
   stateTransitionOnTimeout[6]      = "Wait";

   stateName[7]                     = "Wait";
   stateTimeoutValue[7]             = 0.3;
   stateScript[7]                   = "onReloadWait";
   stateTransitionOnTimeout[7]      = "Reloaded";

   stateName[8]                     = "FireLoadCheckA";
   stateScript[8]                   = "TT_onLoadCheck";
   stateTimeoutValue[8]             = 0.01;
   stateTransitionOnTimeout[8]      = "FireLoadCheckB";

   stateName[9]                     = "FireLoadCheckB";
   stateTransitionOnLoaded[9]       = "Ready";
   stateTransitionOnAmmo[9]         = "Reload";
   stateTransitionOnNoAmmo[9]       = "Empty";

   stateName[10]                    = "Empty";
   stateTransitionOnLoaded[10]      = "Ready";
   stateTransitionOnAmmo[10]        = "ReloadStart";
   stateTransitionOnTriggerDown[10] = "FireCheckA";

   stateName[11]                    = "EmptyFire";
   stateScript[11]                  = "TT_onEmptyFire";
   stateTransitionOnLoaded[11]      = "Ready";
   stateTransitionOnAmmo[11]        = "ReloadStart";
   stateTransitionOnTriggerUp[11]   = "Empty";

   stateName[12]                    = "Reloaded";
   stateTimeoutValue[12]            = 0.1;
   stateScript[12]                  = "onReloaded";
   stateTransitionOnTimeout[12]     = "Activate";

   stateName[13]                    = "FireCheckA";
   stateScript[13]                  = "TT_onFireCheck";
   stateTransitionOnTimeout[13]     = "FireCheckB";

   stateName[14]                    = "FireCheckB";
   stateTransitionOnLoaded[14]      = "Fire";
   stateTransitionOnNotLoaded[14]   = "EmptyFire";

   stateName[15]                    = "ReloadStart";
   stateTransitionOnTimeout[15]     = "FireLoadCheckA";
   stateTimeoutValue[15]            = 0.1;
   stateAllowImageChange[15]        = false;
};

function MortarImage::onFire(%this,%obj,%slot)
{
	%obj.playThread(2, plant);
	%dist = 80;
	%range = 200*getWord(%obj.getScale(),2);
	%start = %obj.getEyePoint();
	%fvec = %obj.getForwardVector();
	%fX = getWord(%fvec,0);
	%fY = getWord(%fvec,1);
	%evec = %obj.getEyeVector();
	%eX = getWord(%evec,0);
	%eY = getWord(%evec,1);
	%eZ = getWord(%evec,2);
	%eXY = mSqrt(%eX*%eX+%eY*%eY);
	%aimVec = %fX*%eXY SPC %fY*%eXY SPC %eZ;
	%end = vectorAdd(%start,vectorScale(%aimVec,%range));
	%ray = containerRayCast(%start,%end,$TypeMasks::All,%obj);
	%target = firstWord(%ray);
	if(isObject(%target))
	{
	%pos = posFromRaycast(%ray);
	%dist = vectorDist(%obj.getPosition(),%pos);
	}
	%initVelocity = vectorScale(%obj.getMuzzleVector(%slot),15);
	%initVelocity = vectorAdd(%initVelocity,getRandom(0,2) / 4 SPC getRandom(0,2) / 4 SPC %dist / 4);
}

function MortarImage::onMount(%this, %obj, %slot)
{
	%obj.playThread(1, armReadyBoth);
}

function MortarImage::onUnMount(%this, %obj, %slot)
{
	fixArmReady(%obj);
}

function MortarImage::onReloadStart(%this,%obj,%slot)
{
	%obj.playThread(2, shiftDown);
	serverPlay3D(block_MoveBrick_Sound,%obj.getPosition());
}

function MortarImage::onReloadWait(%this,%obj,%slot)
{
	%obj.playThread(2, plant);
	serverPlay3D(block_MoveBrick_Sound,%obj.getPosition());
}

function MortarImage::onReloaded(%this,%obj,%slot)
{
	%this.TT_reload(%obj, %slot, block_MoveBrick_Sound);
}
