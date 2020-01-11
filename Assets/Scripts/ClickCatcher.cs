using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using System;
using System.IO;
using System.Text;

public class ClickCatcher : MonoBehaviour
{
    //public CatcherKeys CK;
    //public Toggle TotalSwitch;
    public NetWriter theNW;
    public Image SignalLight;
    public SkillsLink slink;
    private KeyCode Code_G;
    private KeyCode Code_C;
    private KeyCode Code_R;
    private KeyCode Code_T;
    private KeyCode Code_F;
    private KeyCode Code_D;
    private KeyCode Code_E;
    private KeyCode Code_Y;
    private KeyCode Code_S;

    private void OnEnable()
    {
        Code_G = (KeyCode)System.Enum.Parse(typeof(KeyCode), PlayerPrefs.GetString("Mapped_G", "G"));
        Code_C = (KeyCode)System.Enum.Parse(typeof(KeyCode), PlayerPrefs.GetString("Mapped_C", "C"));
        Code_R = (KeyCode)System.Enum.Parse(typeof(KeyCode), PlayerPrefs.GetString("Mapped_R", "R"));
        Code_T = (KeyCode)System.Enum.Parse(typeof(KeyCode), PlayerPrefs.GetString("Mapped_T", "T"));
        Code_F = (KeyCode)System.Enum.Parse(typeof(KeyCode), PlayerPrefs.GetString("Mapped_F", "F"));
        Code_D = (KeyCode)System.Enum.Parse(typeof(KeyCode), PlayerPrefs.GetString("Mapped_D", "D"));
        Code_E = (KeyCode)System.Enum.Parse(typeof(KeyCode), PlayerPrefs.GetString("Mapped_E", "E"));
        Code_Y = (KeyCode)System.Enum.Parse(typeof(KeyCode), PlayerPrefs.GetString("Mapped_Y", "Y"));
        Code_S = (KeyCode)System.Enum.Parse(typeof(KeyCode), PlayerPrefs.GetString("Mapped_S", "S"));
    }

    void Update()
    {
        if (Input.GetKeyDown(Code_G))
        {
            //Debug.Log("G Pressed");
            if (slink.KeyGSkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyGSkill);
                theNW.AddClickData(cd);
                //Debug.Log("G Logged");
            }
        }
        if (Input.GetKeyDown(Code_C))
        {
            if (slink.KeyCSkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyCSkill);
                theNW.AddClickData(cd);
            }
        }
        if (Input.GetKeyDown(Code_R))
        {
            if (slink.KeyRSkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyRSkill);
                theNW.AddClickData(cd);
            }
        }
        if (Input.GetKeyDown(Code_T))
        {
            if (slink.KeyTSkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyTSkill);
                theNW.AddClickData(cd);
            }
        }
        if (Input.GetKeyDown(Code_F))
        {
            if (slink.KeyFSkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyFSkill);
                theNW.AddClickData(cd);
            }
        }
        if (Input.GetKeyDown(Code_D))
        {
            if (slink.KeyDSkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyDSkill);
                theNW.AddClickData(cd);
            }
        }
        if (Input.GetKeyDown(Code_E))
        {
            if (slink.KeyESkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyESkill);
                theNW.AddClickData(cd);
            }
        }
        if (Input.GetKeyDown(Code_Y))
        {
            if (slink.KeyYSkill != null)
            {
                ClickData cd = new ClickData();
                cd.Ksetdata(slink.KeyYSkill);
                theNW.AddClickData(cd);
            }
        }
        if (Input.GetKeyDown(Code_S))
        {
            ClickData cd = new ClickData();
            cd.Ksetdata(SkillCode.FireStop);
            theNW.AddClickData(cd);
        }
        if (Input.GetMouseButtonDown(0))
        {
            Vector2 mp = Camera.main.ScreenToWorldPoint(Input.mousePosition);
            ClickData cd = new ClickData();
            if (Physics2D.OverlapPoint(mp) && Physics2D.OverlapPoint(mp).GetComponent<HPScript>() != null)
                cd.Ssetdata(Physics2D.OverlapPoint(mp).gameObject.name);
            else
                cd.Msetdata(MButton.left, mp.x, mp.y); //Debug.Log("MButton Left Clicked X:" + mp.x);
            theNW.AddClickData(cd);
        }
        if (Input.GetMouseButtonDown(1))
        {
            Vector2 mp = Camera.main.ScreenToWorldPoint(Input.mousePosition);
            ClickData cd = new ClickData();
            cd.Msetdata(MButton.right, mp.x, mp.y);
            theNW.AddClickData(cd);
        }
    }
}
