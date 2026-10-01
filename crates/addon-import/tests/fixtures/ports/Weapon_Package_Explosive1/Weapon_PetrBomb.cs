// The firebomb: it bursts into embers that sear the players near them.
datablock AudioProfile(standinSizzleSound)
{
   filename = "./sizzle.wav";
   description = AudioClosest3d;
   preload = true;
};

AddDamageType("StandinFire", '%1 burned', '%2 burned %1', 0.5, 0);

datablock ExplosionData(standinFirebombExplosion)
{
   lifeTimeMS = 100;
   soundProfile = standinBoomSound;
   damageRadius = 3;
   radiusDamage = 10;
};

datablock ExplosionData(standinSearExplosion)
{
   lifeTimeMS = 100;
};

datablock ProjectileData(tMolotovProjectile)
{
   projectileShapeName = "./firebomb.dts";
   directDamage = 5;
   directDamageType = $DamageType::StandinFire;
   radiusDamageType = $DamageType::StandinFire;
   explosion = standinFirebombExplosion;
   muzzleVelocity = 30;
   velInheritFactor = 0;
   explodeOnPlayerImpact = true;
   explodeOnDeath = true;
   armingDelay = 0;
   lifetime = 3000;
   fadeDelay = 2900;
   isBallistic = true;
};

// The embers burn only through their pulses, so the test reads each sear.
datablock ProjectileData(tierfireRoastProjectile)
{
   directDamage = 0;
   directDamageType = $DamageType::StandinFire;
   radiusDamageType = $DamageType::StandinFire;
   muzzleVelocity = 10;
   velInheritFactor = 0;
   armingDelay = 4900;
   lifetime = 5000;
   fadeDelay = 4900;
   bounceElasticity = 0.5;
   isBallistic = true;
   explodeOnDeath = true;

   PrjLoop_enabled = true;
   PrjLoop_maxTicks = 4;
   PrjLoop_tickTime = 250;
};

datablock ProjectileData(tierfirePlayerProjectile)
{
   explosion = standinSearExplosion;
   lifetime = 30;
   fadeDelay = 30;
   explodeOnDeath = true;
};

datablock ItemData(tMolotovItem : tierfragGrenadeItem)
{
   shapeFile = "./firebomb.dts";
   uiName = "Stand-in Firebomb";
   image = tMolotovImage;
   TT_ammoType = "molNades";
};

datablock ShapeBaseImageData(tMolotovImage)
{
   shapeFile = "./firebomb.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = tMolotovItem;
   projectile = tMolotovProjectile;
   projectileType = Projectile;
   armReady = true;

   stateName[0]                    = "Activate";
   stateTimeoutValue[0]            = 0.02;
   stateTransitionOnTimeout[0]     = "Ready";

   stateName[1]                    = "Ready";
   stateTimeoutValue[1]            = 0.3;
   stateTransitionOnTimeout[1]     = "Ready";
   stateTransitionOnTriggerDown[1] = "Charge";
   stateWaitForTimeout[1]          = false;
   stateScript[1]                  = "onReady";

   stateName[2]                    = "Charge";
   stateTimeoutValue[2]            = 0.1;
   stateTransitionOnTimeout[2]     = "Armed";
   stateScript[2]                  = "onCharge";

   stateName[3]                    = "Armed";
   stateTimeoutValue[3]            = 0.3;
   stateTransitionOnTimeout[3]     = "Armed";
   stateTransitionOnTriggerUp[3]   = "Fire";
   stateWaitForTimeout[3]          = false;
   stateScript[3]                  = "onArmed";

   stateName[4]                    = "Fire";
   stateTimeoutValue[4]            = 0.4;
   stateTransitionOnTimeout[4]     = "Activate";
   stateFire[4]                    = true;
   stateScript[4]                  = "onFire";
   stateAllowImageChange[4]        = false;
};

function tMolotovProjectile::onExplode(%this, %obj, %pos)
{
   Parent::onExplode(%this, %obj, %pos);
   %shrapData = tierfireRoastProjectile;
   %shards = getRandom(2, 3);
   for(%n = 0; %n < %shards; %n++)
   {
      %x = (getRandom(-2, 2) - 0.25) * 2;
      %y = (getRandom(-1, 3) - 0.25) * 2;
      %z = (getRandom(0, 2) - 0.25) * 2;
      %velocity = %x SPC %y SPC %z;
      %ember = new Projectile()
      {
         dataBlock = %shrapData;
         initialVelocity = %velocity;
         initialPosition = %pos;
         sourceObject = %obj.sourceObject;
         client = %obj.client;
      };
      MissionCleanup.add(%ember);
   }
}

function tierfireRoastProjectile::PrjLoop_onTick(%this, %obj)
{
   %count = 0;
   initContainerRadiusSearch(%obj.getPosition(), 3, $TypeMasks::PlayerObjectType);
   while(%o = ContainerSearchNext())
   {
      %target[%count] = %o;
      %count++;
   }
   for(%i = 0; %i < %count; %i++)
   {
      %target = %target[%i];
      %dmg = 5;
      if(miniGameCanDamage(%obj.client, %target) == 1)
      {
         %target.spawnExplosion(tierfirePlayerProjectile, %target.getScale(), %obj);
         if(isObject(%target.client))
            %target.client.play2D(standinSizzleSound);
         %target.damage(%obj, %target.getPosition(), %dmg, $DamageType::StandinFire);
         if(!$Pref::Server::TT::MolNadeTargetBugfix)
            break;
      }
   }
}

function tMolotovImage::onArmed(%this, %obj, %slot)
{
   if(%obj.getImageAmmo(0))
   {
      %obj.playThread(2, spearReady);
   }
   %obj.setImageAmmo(0, 0);
}

function tMolotovImage::onFire(%this, %obj, %slot)
{
   if(!%this.TT_needsAmmo(%obj))
   {
      %p = Parent::onFire(%this, %obj, %slot);
      %this.TT_decrementAmmo(%obj);
      %obj.playThread(2, spearThrow);
      serverPlay3D(standinTossSound, %obj.getPosition());
   }
   if(%this.TT_needsAmmo(%obj))
   {
      %obj.unMountImage(0);
   }
   %obj.setImageAmmo(0, 1);
   return %p;
}

function tMolotovImage::onReady(%this, %obj, %slot)
{
   if(%this.TT_needsAmmo(%obj))
   {
      %obj.unMountImage(0);
   }
}

function tMolotovImage::onCharge(%this, %obj, %slot)
{
   if(%this.TT_needsAmmo(%obj))
   {
      %obj.unMountImage(0);
   }
   else
      %obj.playThread(2, shiftDown);
}
