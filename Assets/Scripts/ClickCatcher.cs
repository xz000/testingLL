using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using System;
using System.IO;
using System.Text;

public class ClickCatcher : MonoBehaviour {
    public CatcherKeys CK;
    public Toggle TotalSwitch;
    public NetWriter theNW;
    public Image SignalLight;
    public SkillsLink slink;

    void Update () {
        if (!TotalSwitch.isOn)
            return;
        if (Input.GetKeyDown(KeyCode.G))
        {
            if (slink.KeyGSkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyGSkill);
                theNW.L2S.Add(cd);
            }
        }
        if (Input.GetKeyDown(KeyCode.C))
        {
            if (slink.KeyCSkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyCSkill);
                theNW.L2S.Add(cd);
            }
        }
        if (Input.GetKeyDown(KeyCode.R))
        {
            if (slink.KeyRSkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyRSkill);
                theNW.L2S.Add(cd);
            }
        }
        if (Input.GetKeyDown(KeyCode.T))
        {
            if (slink.KeyTSkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyTSkill);
                theNW.L2S.Add(cd);
            }
        }
        if (Input.GetKeyDown(KeyCode.F))
        {
            if (slink.KeyFSkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyFSkill);
                theNW.L2S.Add(cd);
            }
        }
        if (Input.GetKeyDown(KeyCode.D))
        {
            if (slink.KeyDSkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyDSkill);
                theNW.L2S.Add(cd);
            }
        }
        if (Input.GetKeyDown(KeyCode.E))
        {
            if (slink.KeyESkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyESkill);
                theNW.L2S.Add(cd);
            }
        }
        if (Input.GetKeyDown(KeyCode.Y))
        {
            if (slink.KeyYSkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyYSkill);
                theNW.L2S.Add(cd);
            }
        }
        if (Input.GetKeyDown(KeyCode.S))
        {
            ClickData cd = new ClickData();
            cd.Ksetdata(SkillCode.FireStop);
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
