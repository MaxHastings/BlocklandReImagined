// Stand-in crit sounds and burst, under the datablock names weapons test for.
datablock AudioProfile(CritHitSound)
{
	filename = "./crit_hit.wav";
	description = AudioClose3d;
	preload = true;
};

datablock AudioProfile(CritRecieveSound)
{
	filename = "./crit_received.wav";
	description = AudioClosest3d;
	preload = true;
};

datablock AudioDescription(StandinQuietDescription : AudioClosest3d)
{
	volume = 0.5;
};

datablock AudioProfile(CritFireSound)
{
	filename = "./crit_fire.wav";
	description = StandinQuietDescription;
	preload = true;
};

datablock ParticleData(CritParticle)
{
	lifetimeMS = 400;
	textureName = "./critical";
	colors[0] = "1 1 0 1";
	colors[1] = "1 1 0 0";
	sizes[0] = 1.0;
	sizes[1] = 0.5;
	times[0] = 0.0;
	times[1] = 1.0;
};

datablock ParticleEmitterData(CritEmitter)
{
	ejectionPeriodMS = 40;
	ejectionVelocity = 0.0;
	ejectionOffset = 1.5;
	velocityVariance = 0.0;
	thetaMin = 0;
	thetaMax = 0;
	lifeTimeMS = 120;
	particles = "CritParticle";
	emitterNode = GenericEmitterNode;
	pointEmitterNode = TenthEmitterNode;
	uiName = "Stand-in Crit";
};

datablock ExplosionData(CritExplosion)
{
	lifeTimeMS = 1500;
	emitter[0] = CritEmitter;
};

datablock ProjectileData(CritProjectile)
{
	explosion = CritExplosion;
	lifetime = 10;
	explodeOnDeath = true;
	uiName = "Stand-in Crit";
};
