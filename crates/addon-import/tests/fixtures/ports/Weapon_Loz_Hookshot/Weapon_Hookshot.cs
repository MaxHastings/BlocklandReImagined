// Stand-in for the Weapon_Loz_Hookshot port tests (CC0): a gun whose shot
// pulls the shooter to where it strikes. Its own numbers; the function
// shapes the port reads.
datablock ProjectileData(hookshotProjectile)
{
   directDamage = 0;
   muzzleVelocity = 150;
   velInheritFactor = 0;
   lifetime = 1500;
   fadeDelay = 1400;
   isBallistic = true;
   gravityMod = 0;
};

datablock ItemData(hookshotItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./gun.dts";
   uiName = "Stand-in Hookshot";
   image = hookshotImage;
   canDrop = true;
};

datablock ShapeBaseImageData(hookshotImage)
{
   shapeFile = "./gun.dts";
   mountPoint = 0;
   correctMuzzleVector = true;
   className = "WeaponImage";
   item = hookshotItem;
   projectile = hookshotProjectile;
   projectileType = Projectile;
   armReady = true;

   stateName[0]                    = "Activate";
   stateTimeoutValue[0]            = 0.1;
   stateTransitionOnTimeout[0]     = "Ready";

   stateName[1]                    = "Ready";
   stateTransitionOnTriggerDown[1] = "Fire";

   stateName[2]                    = "Fire";
   stateTransitionOnTimeout[2]     = "Wait";
   stateTimeoutValue[2]            = 0.1;
   stateFire[2]                    = true;
   stateScript[2]                  = "onFire";
   stateWaitForTimeout[2]          = true;

   stateName[3]                    = "Wait";
   stateTimeoutValue[3]            = 0.4;
   stateWaitForTimeout[3]          = true;
   stateTransitionOnTimeout[3]     = "Ready";
};

function hookshotProjectile::onCollision(%this, %obj, %col, %fade, %pos, %normal)
{
   %player = %obj.client.player;
   if(%col.getClassName() $= "Player" || %col.getClassName() $= "WheeledVehicle")
      pushPlayerToObj2(%player, %col);
   else
      pushPlayerToObj(%player, %pos);
}

function pushPlayerToObj(%player, %cpos)
{
   if(!isObject(%player))
      return;
   %vec = VectorSub(%cpos, %player.getPosition());
   %len = VectorLen(%vec);
   if(%len < 4)
      return;
   %vec = VectorNormalize(%vec);
   if(%len < 12)
      %vec = VectorScale(%vec, 25);
   if(%len > 13)
      %vec = VectorScale(%vec, 40);
   %player.setVelocity(%vec);
   cancel(%player.pptoTick);
   %player.pptoTick = schedule(125, 0, "pushPlayerToObj", %player, %cpos);
}

function pushPlayerToObj2(%player, %target)
{
   if(!isObject(%player) || !isObject(%target))
      return;
   %cpos = %target.getPosition();
   if(%target.getClassName() $= "FlyingVehicle")
      %cpos = VectorAdd(%cpos, "0 0 1");
   pushPlayerToObj(%player, %cpos);
   cancel(%player.pptoTick);
   %player.pptoTick = schedule(125, 0, "pushPlayerToObj2", %player, %target);
}

function serverCmdDegrapple(%client)
{
   cancel(%client.player.pptoTick);
}
