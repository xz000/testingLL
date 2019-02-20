using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillT : MonoBehaviour
{
    public Toggle T1;
    public Toggle T1a;
    public Toggle T1b;
    public Toggle T2;
    public Toggle T2a;
    public Toggle T2b;
    public Toggle T3;
    public Toggle T3a;
    public Toggle T3b;
    public Image IconT;
    GameObject Soldier;

    public void SetT()
    {
        AllTOff();
        IconT.GetComponent<CooldownImage>().enabled = true;
        if (T1.isOn && T1a.isOn)
        {
            if (GetComponent<SkillsLink>().mySoldier != null)
                IconT.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<TestSkillLeech>().CalcFA;
            GetComponent<SkillsLink>().KeyTSkill = SkillCode.TestSkillLeech;
            return;
        }
        if (T1.isOn && T1b.isOn)
        {
            if (GetComponent<SkillsLink>().mySoldier != null)
                IconT.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillT1b>().CalcFA;
            GetComponent<SkillsLink>().KeyTSkill = SkillCode.SkillT1b;
            return;
        }
        if (T2.isOn && T2a.isOn)
        {
            if (GetComponent<SkillsLink>().mySoldier != null)
                IconT.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillT2>().CalcFA;
            GetComponent<SkillsLink>().KeyTSkill = SkillCode.SkillT2;
            return;
        }
        if (T2.isOn && T2b.isOn)
        {
            if (GetComponent<SkillsLink>().mySoldier != null)
                IconT.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillT2b>().CalcFA;
            GetComponent<SkillsLink>().KeyTSkill = SkillCode.SkillT2b;
            return;
        }
        if (T3.isOn && T3a.isOn)
        {
            if (GetComponent<SkillsLink>().mySoldier != null)
                IconT.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillT3>().CalcFA;
            GetComponent<SkillsLink>().KeyTSkill = SkillCode.SkillT3;
            return;
        }
        if (T3.isOn && T3b.isOn)
        {
            if (GetComponent<SkillsLink>().mySoldier != null)
                IconT.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillT3b>().CalcFA;
            GetComponent<SkillsLink>().KeyTSkill = SkillCode.SkillT3b;
            return;
        }
    }

    public void AllTOff()
    {
        IconT.GetComponent<CooldownImage>().enabled = false;
        GetComponent<SkillsLink>().KeyTSkill = null;
    }
}
