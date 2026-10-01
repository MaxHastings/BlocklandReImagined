// The grenade launcher: a bouncing grenade that bursts on a player.
datablock ProjectileData(GrenaderProjectile)
{
   projectileShapeName = "./shell.dts";
   directDamage = 80;
   directDamageType = $DamageType::StandinRocketDirect;
   radiusDamageType = $DamageType::StandinRocketDirect;
   explosion = standinRocketExplosion;
   muzzleVelocity = 60;
   velInheritFactor = 0;
   armingDelay = 0;
   lifetime = 3000;
   fadeDelay = 3000;
   bounceElasticity = 0.6;
   bounceFriction = 0.3;
   isBallistic = true;
   explodeOnPlayerImpact = true;
   explodeOnDeath = true;
};

datablock ItemData(GrenaderItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./launcher.dts";
   uiName = "Stand-in Grenade Launcher";
   image = GrenaderImage;
   canDrop = true;
   TT_ammoType = "Bomb";
   TT_reloads = true;
   TT_maxAmmo = 1;
   TT_alwaysReloadPref = "Ex";
};

datablock ShapeBaseImageData(GrenaderImage)
{
   shapeFile = "./launcher.dts";
   mountPoint = 0;
   correctMuzzleVector = true;
   className = "WeaponImage";
   item = GrenaderItem;
   projectile = GrenaderProjectile;
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

function GrenaderImage::onFire(%this,%obj,%slot)
{
   %this.TT_decrementAmmo(%obj);
   %this.TT_displayAmmo(%obj);
   %obj.playThread(2, plant);
   return Parent::onFire(%this,%obj,%slot);
}

function GrenaderImage::onReloadStart(%this,%obj,%slot)
{
   if($Pref::Server::TT::DeathStopAnims && %obj.getDamagePercent() >= 1.0)
      return;
   %obj.playThread(2, shiftDown);
   serverPlay3D(block_MoveBrick_Sound,%obj.getPosition());
   %this.TT_displayAmmo(%obj);
}

function GrenaderImage::onReloadWait(%this,%obj,%slot)
{
   if($Pref::Server::TT::DeathStopAnims && %obj.getDamagePercent() >= 1.0)
      return;
   %obj.playThread(2, plant);
   serverPlay3D(block_MoveBrick_Sound,%obj.getPosition());
   %this.TT_displayAmmo(%obj);
}

function GrenaderImage::onReloaded(%this,%obj,%slot)
{
   if($Pref::Server::TT::DeathStopAnims && %obj.getDamagePercent() >= 1.0)
      return %obj.setImageLoaded(%slot, 1);
   %this.TT_reload(%obj, %slot, block_MoveBrick_Sound);
   %this.TT_displayAmmo(%obj);
}
