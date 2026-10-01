datablock AudioProfile(stimpackHealSound)
{
   filename = "./mend.wav";
   description = AudioClose3d;
   preload = true;
};

datablock ItemData(stimpackItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./shot.dts";
   uiName = "Stand-in Booster";
   image = stimpackImage;
   canDrop = true;
};

datablock ProjectileData(stimpackProjectile)
{
   projectileShapeName = "";
   directDamage = 0;
   impactImpulse = 0;
   verticalImpulse = 0;
   muzzleVelocity = 80;
   velInheritFactor = 0;
   armingDelay = 0;
   lifetime = 150;
   isBallistic = false;
   gravityMod = 0.0;
};

datablock ShapeBaseImageData(stimpackImage)
{
   shapeFile = "./shot.dts";
   mountPoint = 0;
   minShotTime = 4000;
   className = "WeaponImage";
   item = stimpackItem;
   armReady = true;
   altTriggerEnabled = true;

   stateName[0] = "Ready";
   stateTransitionOnTriggerDown[0] = "Fire";

   stateName[1] = "Fire";
   stateTransitionOnTimeout[1] = "Ready";
   stateScript[1] = "onFire";
   stateTimeoutValue[1] = 0.5;
};

function stimpackProjectile::onCollision(%this, %obj, %col, %pos, %fade)
{
   TT_projectileHeal(%obj, %col, 32);
   Parent::onCollision(%this, %obj, %col, %pos, %fade);
}

function stimRechargedNotice(%obj)
{
   if(%obj.getDamagePercent() >= 1.0)
      return;
   centerprint(%obj.client, "\c2Booster ready.", 3);
   serverPlay3D(medigunReloadSound, %obj.getPosition());
}

function stimpackImage::onFire(%this, %obj, %slot)
{
   if((%obj.lastStimTime + %this.minShotTime) > getSimTime())
   {
      centerprint(%obj.client, "\c0Booster still charging.", 2);
      return;
   }
   %obj.playThread(2, shiftUp);
   %obj.lastStimTime = getSimTime();
   serverPlay3D(stimpackHealSound, %obj.getPosition());
   schedule(4000, %obj, stimRechargedNotice, %obj);
   if(%obj.getDamageLevel() >= 25)
   {
      %obj.spawnExplosion(healCrossProjectile, %obj.getScale());
      %obj.emote(medigunHealImage);
   }
   %obj.setDamageLevel(%obj.getDamageLevel() - 25);
}

function stimpackImage::onAltTrigger(%this, %player, %playerDB, %triggerSlot, %val)
{
   if(%val && %triggerSlot == 4)
   {
      %player.playThread(2, shiftUp);
      serverPlay3D(stimpackHealSound, %player.getPosition());

      %projectile = StimpackProjectile;
      %vector = %player.getMuzzleVector(0);
      %objectVelocity = %player.getVelocity();
      %vector1 = VectorScale(%vector, %projectile.muzzleVelocity);
      %vector2 = VectorScale(%objectVelocity, %projectile.velInheritFactor);
      %velocity = VectorAdd(%vector1, %vector2);
      %p = new Projectile()
      {
         dataBlock = %projectile;
         initialVelocity = %velocity;
         initialPosition = %player.getMuzzlePoint(0);
         sourceObject = %player;
         sourceSlot = 0;
         client = %player.client;
      };
      MissionCleanup.add(%p);
      return %p;
   }
}
