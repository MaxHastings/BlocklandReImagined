datablock AudioProfile(HealCrossSound)
{
   filename = "./mend.wav";
   description = AudioClosest3d;
   preload = true;
};

datablock ParticleData(HealCrossParticle)
{
   dragCoefficient = 4.0;
   gravityCoefficient = -0.5;
   lifetimeMS = 1200;
   textureName = "base/data/particles/dot";
   colors[0] = "0.2 1 0.2 1";
   colors[1] = "0.2 1 0.2 0";
   sizes[0] = 0.6;
   sizes[1] = 0.6;
   times[0] = 0.0;
   times[1] = 1.0;
};

datablock ParticleEmitterData(HealCrossEmitter)
{
   ejectionPeriodMS = 40;
   ejectionVelocity = 1.0;
   ejectionOffset = 1.0;
   thetaMin = 0;
   thetaMax = 180;
   phiVariance = 360;
   lifeTimeMS = 80;
   particles = "HealCrossParticle";
};

datablock ExplosionData(HealCrossExplosion)
{
   lifeTimeMS = 800;
   emitter[0] = HealCrossEmitter;
   soundProfile = HealCrossSound;
};

datablock ProjectileData(HealCrossProjectile)
{
   explosion = HealCrossExplosion;
   armingDelay = 0;
   lifetime = 10;
   explodeOnDeath = true;
};
