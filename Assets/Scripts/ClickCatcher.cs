﻿using System.Collections;
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
        if (Input.GetKeyDown(KeyCode.G))
        {
            ClickData cd = new ClickData();
            cd.Ksetdata(SkillCode.TestSkill01);
            theNW.L2S.Add(cd);
        }
        if (Input.GetKeyDown(KeyCode.C))
        {
            ClickData cd = new ClickData();
            cd.Ksetdata(SkillCode.SkillC3);
            theNW.L2S.Add(cd);
        }
        if (Input.GetKeyDown(KeyCode.R))
        {
            ClickData cd = new ClickData();
            cd.Ksetdata(SkillCode.TestSkill02);
            theNW.L2S.Add(cd);
        }
        if (Input.GetKeyDown(KeyCode.F))
        {
            ClickData cd = new ClickData();
            cd.Ksetdata(SkillCode.TestSkill03);
            theNW.L2S.Add(cd);
        }
        if (Input.GetKeyDown(KeyCode.D))
        {
            ClickData cd = new ClickData();
            cd.Ksetdata(SkillCode.TestSkillLightning);
            theNW.L2S.Add(cd);
        }
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
