using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using System;
using System.IO;
using System.Text;

public class ClickCatcher : MonoBehaviour {
    public Toggle TotalSwitch;
    public string FileName;
    public string txtPath;
    //public static List<ClickData> LS = new List<ClickData>();
    public NetWriter theNW;
    public Image SignalLight;

    void Update () {
        if (!TotalSwitch.isOn)
            return;
        if (Input.GetMouseButtonDown(0))
        {
            Vector2 mp = Camera.main.ScreenToWorldPoint(Input.mousePosition);
            ClickData cd = new ClickData();
            cd.Msetdata(MButton.left, mp.x, mp.y);
            theNW.L2S.Add(cd);
        }
        if (Input.GetMouseButtonDown(1))
        {
            Vector2 mp = Camera.main.ScreenToWorldPoint(Input.mousePosition);
            ClickData cd = new ClickData();
            cd.Msetdata(MButton.right, mp.x, mp.y);
            theNW.L2S.Add(cd);
        }
    }
}
