// Stand-in for the Tool_GrappleRope port tests (CC0): a launcher whose hook
// ropes its holder to where it lands while the trigger is held. Its own
// numbers; the function shapes the port reads.
datablock ParticleData(ChainTrailParticle)
{
   lifetimeMS = 40;
   textureName = "base/data/particles/dot";
   colors[0] = "0.3 0.3 0.3 1";
   sizes[0] = 0.1;
   times[0] = 0.0;
};

datablock ParticleEmitterData(ChainTrailEmitter)
{
   ejectionPeriodMS = 2;
   ejectionVelocity = 0;
   particles = ChainTrailParticle;
};

datablock ProjectileData(ChainProjectile)
{
   particleEmitter = ChainTrailEmitter;
   muzzleVelocity = 150;
   lifetime = 2000;
   isBallistic = true;
   gravityMod = 0;
};

datablock ProjectileData(GrappleRopeProjectile)
{
   muzzleVelocity = 150;
   velInheritFactor = 0;
   lifetime = 2000;
   fadeDelay = 1900;
   isBallistic = true;
   gravityMod = 0;
};

datablock ItemData(GrappleRope)
{
   category = "Tool";
   className = "Tool";
   shapeFile = "./launcher.dts";
   uiName = "Stand-in Rope";
   image = GrappleRopeImage;
   canDrop = true;
};

datablock ShapeBaseImageData(GrappleRopeImage)
{
   shapeFile = "./launcher.dts";
   mountPoint = 0;
   correctMuzzleVector = true;
   className = "WeaponImage";
   item = GrappleRope;
   projectile = GrappleRopeProjectile;
   projectileType = Projectile;
   armReady = true;

   stateName[0]                    = "Activate";
   stateTimeoutValue[0]            = 0.1;
   stateTransitionOnTimeout[0]     = "Ready";
   stateScript[0]                  = "onRelease";

   stateName[1]                    = "Ready";
   stateTransitionOnTriggerDown[1] = "Fire";

   stateName[2]                    = "Fire";
   stateTransitionOnTimeout[2]     = "Hold";
   stateTimeoutValue[2]            = 0.02;
   stateFire[2]                    = true;
   stateScript[2]                  = "onFire";
   stateWaitForTimeout[2]          = true;

   stateName[3]                    = "Hold";
   stateTimeoutValue[3]            = 0.02;
   stateScript[3]                  = "onHold";
   stateTransitionOnTimeout[3]     = "Hold";
   stateTransitionOnTriggerUp[3]   = "Release";

   stateName[4]                    = "Release";
   stateTimeoutValue[4]            = 0.02;
   stateTransitionOnTimeout[4]     = "Ready";
   stateScript[4]                  = "onRelease";
};

function GrappleRopeProjectile::onCollision(%this, %obj, %col, %fade, %pos, %normal)
{
   if(!%col.GrappleRopeTarget && !$Pref::Server::GrappleRopeAnywhere)
      return;
   %pl = %obj.client.player;
   %ppos = %pl.getPosition();
   %block = ContainerRayCast(vectorAdd(%ppos, "0 0 1.5"), %pos, $TypeMasks::StaticObjectType);
   if(!%block || %block == %col)
   {
      %pl.ropeLength = vectorLen(vectorSub(%pl.getPosition(), %pos));
      %pl.GrappleRopePos = %pos;
   }
}

function GrappleRopeImage::onHold(%this, %obj, %slot)
{
   if(%obj.GrappleRopePos $= "")
      return;
   GrappleRope(%obj, %obj.GrappleRopePos);
   %norm = VectorNormalize(VectorSub(%obj.GrappleRopePos, %obj.getMuzzlePoint(%slot)));
   %p = new Projectile()
   {
      dataBlock = ChainProjectile;
      initialVelocity = VectorScale(%norm, 120);
      initialPosition = %obj.getMuzzlePoint(%slot);
      sourceObject = %obj;
      sourceSlot = %slot;
      client = %obj.client;
   };
   MissionCleanup.add(%p);
}

function GrappleRopeImage::onRelease(%this, %obj, %slot)
{
   %obj.GrappleRopePos = "";
}

function GrappleRope(%obj, %pos)
{
   %vel = %obj.getVelocity();
   %out = vectorSub(%obj.getPosition(), %pos);
   if(vectorLen(%out) > %obj.ropeLength)
      %vel = vectorSub(%vel, vectorScale(vectorNormalize(%out), vectorDot(%vel, vectorNormalize(%out))));
   %obj.setVelocity(%vel);
}

$Pref::Server::GrappleRopeAnywhere = 1;
