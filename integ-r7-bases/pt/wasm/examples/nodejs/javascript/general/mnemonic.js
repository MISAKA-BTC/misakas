// ⚠️ TEST-ONLY EXAMPLE — NOT FOR REAL FUNDS.
// The mnemonics, seeds, private keys and xprvs generated or printed below are for
// demonstration only. Never reuse a phrase/key shown here, and never log a real secret:
// anything printed to a console (or saved to a file/CI log) must be treated as compromised.

const kaspa = require('../../../../nodejs/kaspa');
const {
    Mnemonic,
} = kaspa;

kaspa.initConsolePanicHook();

(async () => {

    const mnemonic1 = Mnemonic.random();
    console.log("mnemonic1:", mnemonic1);

    const mnemonic2 = new Mnemonic(mnemonic1.phrase);
    console.log("mnemonic2:", mnemonic2);

    // create a seed with a recovery password ("25th word")
    const seed1 = mnemonic1.toSeed("my_password");
    console.log("seed1:", seed1);

    const seed2 = mnemonic2.toSeed("my_password");
    console.log("seed2:", seed2);

    if (seed1 !== seed2) {
        throw Error("mnemonic restore failure");
    } else {
        console.log("mnemonic restore success");
    }

    // create a seed without a recovery password
    const seed3 = mnemonic1.toSeed();
    console.log("seed3 (no recovery password):", seed3);

})();
