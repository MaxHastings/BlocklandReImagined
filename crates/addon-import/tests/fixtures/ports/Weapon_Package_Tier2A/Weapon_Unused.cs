// Never run: server.cs leaves its exec commented out. Its brick buster is
// bigger than the engine allows, so reading it would stop the import.
datablock ProjectileData(UnusedBusterProjectile)
{
   lifetime = 100;
   brickExplosionImpact = true;
   brickExplosionForce = 100;
   brickExplosionMaxVolume = 655360;
};

datablock ItemData(UnusedStarItem : standinSidearmItem)
{
   uiName = "Stand-in Unused";
};
