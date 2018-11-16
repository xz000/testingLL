using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class HostCook : MonoBehaviour {
    public Text HostNOtext;
    
    public int HostStr2int()
    {
        int ttt;
        int.TryParse(HostNOtext.text, out ttt);
        return ttt;
    }
}
