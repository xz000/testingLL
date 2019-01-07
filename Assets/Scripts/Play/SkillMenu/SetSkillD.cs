using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillD : MonoBehaviour
{
    public Toggle D1;
    public Toggle D2;
    public Toggle D3;
    public Toggle D4;
    //public Toggle D5;
    public Image IconD;
    GameObject Soldier;

    public void SetD()
    {
        if (Soldier == null)
            Soldier = gameObject.GetComponent<SkillsLink>().mySoldier;
        AllDOff();
        if (D1.isOn)
        {
            Soldier.GetComponent<TestSkillLightning>().MyImageScript = IconD.GetComponent<CooldownImage>();
            Soldier.GetComponent<TestSkillLightning>().enabled = true;
            return;
        }
        if (D2.isOn)
        {
            Soldier.GetComponent<SkillD2>().MyImageScript = IconD.GetComponent<CooldownImage>();
            Soldier.GetComponent<SkillD2>().enabled = true;
            return;
        }
        if (D3.isOn)
        {
            Soldier.GetComponent<SkillD3>().MyImageScript = IconD.GetComponent<CooldownImage>();
            Soldier.GetComponent<SkillD3>().enabled = true;
            return;
        }
        if (D4.isOn)
        {
            Soldier.GetComponent<SkillD4>().MyImageScript = IconD.GetComponent<CooldownImage>();
            Soldier.GetComponent<SkillD4>().enabled = true;
            return;
        }
    }

    public void AllDOff()
    {
        IconD.GetComponent<CooldownImage>().enabled = false;
        Soldier.GetComponent<TestSkillLightning>().enabled = false;
        Soldier.GetComponent<TestSkillLightning>().MyImageScript = null;
        Soldier.GetComponent<SkillD2>().enabled = false;
        Soldier.GetComponent<SkillD2>().MyImageScript = null;
        Soldier.GetComponent<SkillD3>().enabled = false;
        Soldier.GetComponent<SkillD3>().MyImageScript = null;
        Soldier.GetComponent<SkillD4>().enabled = false;
        Soldier.GetComponent<SkillD4>().MyImageScript = null;
    }
}
