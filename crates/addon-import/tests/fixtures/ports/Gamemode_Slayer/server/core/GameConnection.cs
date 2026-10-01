// Stand-in (CC0): the death and score shapes the port reads. Not the
// original's code.
package Slayer_GameConnection
{
	function GameConnection::onDeath(%this, %obj, %killer, %type, %area)
	{
		if(%this.getLives() > 0)
		{
			%this.addLives(-1);
			if(%this.getLives() <= 0)
				%this.setDead(1);
		}
		%this.centerPrint("\c5You have \c30 \c5lives left.", 3);
	}

	function GameConnection::setScore(%this, %flag)
	{
		%winner = %mini.victoryCheck_Points();
	}
};
activatePackage(Slayer_GameConnection);
