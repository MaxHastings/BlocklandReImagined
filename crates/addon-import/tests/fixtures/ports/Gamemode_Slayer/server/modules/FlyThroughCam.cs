// Stand-in (CC0): a module that wraps a core function in a package, as the
// original's fly-through camera does. Ports read the core definition.
package Slayer_Stand_In_Module
{
	function Slayer_MinigameSO::preRoundCountdownTick(%this, %ticks)
	{
		%parent = parent::preRoundCountdownTick(%this, %ticks);
		return %parent;
	}
};
activatePackage(Slayer_Stand_In_Module);
