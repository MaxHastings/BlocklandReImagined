// Stand-in for the importer's port tests (CC0). Short bodies with the
// calls the port's patterns look for; none of it is the original's code.

datablock ProjectileData(TrenchDirtProjectile)
{
   muzzleVelocity = 40;
   velInheritFactor = 1;
   lifetime = 350;
   isBallistic = true;
};

datablock ItemData(TrenchShovelItem)
{
   shapeFile = "base/data/shapes/brickweapon.dts";
   uiName = "Stand-in Shovel";
   image = TrenchShovelImage;
   canDrop = true;
};

datablock ItemData(TrenchDirtItem : TrenchShovelItem)
{
   uiName = "Stand-in Dirt";
   image = TrenchDirtImage;
};

datablock ShapeBaseImageData(TrenchShovelImage)
{
   shapeFile = "base/data/shapes/brickweapon.dts";
   mountPoint = 0;
   item = TrenchShovelItem;
   melee = true;
   armReady = true;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.5;
   stateTransitionOnTimeout[0] = "Ready";

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "PreFire";
   stateAllowImageChange[1] = true;

   stateName[2] = "PreFire";
   stateScript[2] = "onPreFire";
   stateAllowImageChange[2] = false;
   stateTimeoutValue[2] = 0.1;
   stateTransitionOnTimeout[2] = "Fire";

   stateName[3] = "Fire";
   stateTransitionOnTimeout[3] = "CheckFire";
   stateTimeoutValue[3] = 0.3;
   stateFire[3] = true;
   stateAllowImageChange[3] = false;
   stateScript[3] = "onFire";
   stateWaitForTimeout[3] = true;

   stateName[4] = "CheckFire";
   stateTransitionOnTriggerUp[4] = "StopFire";
   stateTransitionOnTriggerDown[4] = "PreFire";

   stateName[5] = "StopFire";
   stateTransitionOnTimeout[5] = "Ready";
   stateTimeoutValue[5] = 0.2;
   stateAllowImageChange[5] = false;
   stateWaitForTimeout[5] = true;
};

datablock ShapeBaseImageData(TrenchDirtImage : TrenchShovelImage)
{
   item = TrenchDirtItem;
   melee = false;
   projectile = TrenchDirtProjectile;
   projectileType = Projectile;
};

datablock ShapeBaseImageData(AdminShovelImage)
{
   shapeFile = "base/data/shapes/brickweapon.dts";
   mountPoint = 0;
   melee = true;
   armReady = true;

   stateName[0] = "Activate";
   stateTimeoutValue[0] = 0.05;
   stateTransitionOnTimeout[0] = "Ready";

   stateName[1] = "Ready";
   stateTransitionOnTriggerDown[1] = "Fire";

   stateName[2] = "Fire";
   stateTimeoutValue[2] = 0.04;
   stateTransitionOnTimeout[2] = "Ready";
   stateFire[2] = true;
   stateAllowImageChange[2] = false;
   stateScript[2] = "onFire";
   stateWaitForTimeout[2] = true;
};

datablock ShapeBaseImageData(AdminDirtImage : AdminShovelImage)
{
   projectile = TrenchDirtProjectile;
   projectileType = Projectile;
};

function TrenchShovelImage::onPreFire(%this, %obj, %slot) { %obj.playThread(2, armAttack); }
function TrenchDirtImage::onPreFire(%this, %obj, %slot) { %obj.playThread(2, armAttack); }
function TrenchShovelImage::onFire(%this, %obj, %slot) { TakeChunk(%obj.client, 1); }
function AdminShovelImage::onFire(%this, %obj, %slot) { TakeChunk(%obj.client, 1); }
function TrenchDirtImage::onFire(%this, %obj, %slot)
{
   Parent::onFire(%this, %obj, %slot);
   ShootChunk(%obj.client);
}
function AdminDirtImage::onFire(%this, %obj, %slot)
{
   Parent::onFire(%this, %obj, %slot);
   ShootChunk(%obj.client);
}

function TakeChunk(%client, %take)
{
   %eyeVector = %client.player.getEyeVector();
   %end = vectorScale(%eyeVector, 10);
   %obj.refill(%client, %take, %rayPos);
   %client.player.addVelocity("0 0 2");
}
function ShootChunk(%client)
{
   %a = new fxDtsBrick() { datablock = brick2xCubeDirtData; };
   %b = new fxDtsBrick() { datablock = brick1x1DirtData; };
}
function fxDtsBrick::Refill(%this, %client, %take, %rayPos)
{
   fillPos(%this);
   continueRefill(%client);
}
function fillPos(%pos) { %a = brick32xCubeDirtData; %b = brick1x1DirtData; }
function continueRefill(%client) { %brick.Refill(%client); }
function onTrenchDig(%client) { return %client.trenchDirt >= $TrenchDig::DirtCount; }
function findSmallBrick(%brick, %Pos) { return vectorDist(%Pos, %smallPos); }
function fxDtsBrick::checkBricks(%this)
{
   if(%puzzleComplete >= 3) return 1;
   if(%puzzleComplete >= 7) return 2;
}
function finishRegrouping(%pos) { %db = brick64xCubeDirtData; }
function serverCmdDumpDirt(%client) { %a = brick4xCubeDirtData; %b = brick4x4DirtData; }
function serverCmdSpeedDig(%client) { %client.player.mountImage(AdminShovelImage, 0); }
function serverCmdSpeedPlace(%client) { %client.player.mountImage(AdminDirtImage, 0); }
function serverCmdInfiniteDigging(%client) { %client.isInfiniteMiner = !%client.isInfiniteMiner; }
function GameConnection::updateDirt(%this) { %this.bottomPrint(%this.trenchDirt, -1); }
