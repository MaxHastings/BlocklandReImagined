// The flak cannon: a round that sheds sparks as it flies and a burst of
// them at everything it hits.
datablock ExplosionData(FlakCannonBlastExplosion)
{
   lifeTimeMS = 100;
   soundProfile = standinBoomSound;
   damageRadius = 2;
   radiusDamage = 10;
};

AddDamageType("FlakCannonDirect", '%1 was shredded', '%2 shredded %1', 1, 1);
datablock ProjectileData(FlakCannonProjectile)
{
   projectileShapeName = "./shell.dts";
   directDamage = 60;
   directDamageType = $DamageType::FlakCannonDirect;
   radiusDamageType = $DamageType::FlakCannonDirect;
   explosion = FlakCannonBlastExplosion;
   muzzleVelocity = 50;
   velInheritFactor = 0;
   armingDelay = 1200;
   lifetime = 1200;
   fadeDelay = 1200;
   bounceElasticity = 0.8;
   bounceFriction = 0;
   isBallistic = true;
   gravityMod = 0.25;
   explodeOnPlayerImpact = true;
   explodeOnDeath = true;

   PrjLoop_enabled = true;
   PrjLoop_maxTicks = 150;
   PrjLoop_tickTime = 250;
};

datablock ProjectileData(FlakCannonSparkProjectile)
{
   directDamage = 6;
   directDamageType = $DamageType::FlakCannonDirect;
   radiusDamageType = $DamageType::FlakCannonDirect;
   explosion = FlakCannonBlastExplosion;
   muzzleVelocity = 20;
   velInheritFactor = 1;
   explodeOnDeath = true;
   armingDelay = 0;
   lifetime = 60;
   fadeDelay = 60;
   bounceElasticity = 0.5;
   isBallistic = true;
   gravityMod = 0.5;
};

datablock ItemData(FlakCannonItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./launcher.dts";
   uiName = "Stand-in Flak";
   image = FlakCannonImage;
   canDrop = true;
   TT_ammoType = "Bomb";
   TT_reloads = true;
   TT_maxAmmo = 1;
   TT_alwaysReloadPref = "Ex";
};

datablock ShapeBaseImageData(FlakCannonImage)
{
   shapeFile = "./launcher.dts";
   mountPoint = 0;
   correctMuzzleVector = true;
   className = "WeaponImage";
   item = FlakCannonItem;
   projectile = FlakCannonProjectile;
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
   stateSound[2]                    = standinBoomSound;

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

function FlakCannonProjectile::PrjLoop_onTick(%this, %obj)
{
   %emittedPrj = FlakCannonSparkProjectile;
   %speed = 15;
   %amount = 3;
   PrjLoop_emitPrj(%obj, %emittedPrj, %speed, %amount);
}

function FlakCannonProjectile::onCollision(%this,%obj,%col,%fade,%pos,%normal)
{
   for(%i=0;%i<getRandom(2,4);%i++)
   {
      %a=getRandom(0,360)/360*2*$pi;
      %b=getRandom(0,360)/360*2*$pi;

      %vec=mcos(%a) SPC msin(%a) SPC mcos(%b);

      %p=new Projectile()
      {
         datablock=FlakCannonSparkProjectile;

         initialVelocity=vectorScale(%vec,150);
         initialPosition=%obj.getPosition();

         client=%obj.client;
         sourceObject=%obj.sourceobject;
         sourceSlot=%obj.sourceslot;
      };
      MissionCleanup.add(%p);
   }
   Parent::onCollision(%this,%obj,%col,%fade,%pos,%normal);
}

function FlakCannonImage::onFire(%this, %obj, %slot)
{
   %this.TT_decrementAmmo(%obj);
   %this.TT_displayAmmo(%obj);
   %obj.playThread(2, plant);
   return Parent::onFire(%this,%obj,%slot);
}

function FlakCannonImage::onReloadStart(%this,%obj,%slot)
{
   if($Pref::Server::TT::DeathStopAnims && %obj.getDamagePercent() >= 1.0)
      return;
   %obj.playThread(2, shiftDown);
   serverPlay3D(block_MoveBrick_Sound,%obj.getPosition());
   %this.TT_displayAmmo(%obj);
}

function FlakCannonImage::onReloadWait(%this,%obj,%slot)
{
   if($Pref::Server::TT::DeathStopAnims && %obj.getDamagePercent() >= 1.0)
      return;
   %obj.playThread(2, plant);
   serverPlay3D(block_MoveBrick_Sound,%obj.getPosition());
   %this.TT_displayAmmo(%obj);
}

function FlakCannonImage::onReloaded(%this,%obj,%slot)
{
   if($Pref::Server::TT::DeathStopAnims && %obj.getDamagePercent() >= 1.0)
      return %obj.setImageLoaded(%slot, 1);
   %this.TT_reload(%obj, %slot, block_MoveBrick_Sound);
   %this.TT_displayAmmo(%obj);
}
