using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class SetSkillY : MonoBehaviour
{
    public Toggle Y1;
    public Toggle Y1a;
    public Toggle Y2;
    public Toggle Y2a;
    public Toggle Y2b;
    public Toggle Y3;
    public Toggle Y3a;
    public Toggle Y3b;
    public Toggle Y4;
    public Toggle Y4a;
    public Toggle Y4b;
    public Image IconY;

    public void SetY()
    {
        AllYOff();
        IconY.GetComponent<CooldownImage>().enabled = true;
        if (Y1.isOn)
        {
            if (Y1a.isOn)
            {
                IconY.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillY1>().CalcFA;
                GetComponent<SkillsLink>().KeyYSkill = SkillCode.SkillY1;
                return;
            }
            IconY.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillY1b>().CalcFA;
            GetComponent<SkillsLink>().KeyYSkill = SkillCode.SkillY1b;
            return;
        }
        if (Y2.isOn)
        {
            if (Y2a.isOn)
            {
                IconY.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillY2>().CalcFA;
                GetComponent<SkillsLink>().KeyYSkill = SkillCode.SkillY2;
                return;
            }
            IconY.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SkillY2b>().CalcFA;
            GetComponent<SkillsLink>().KeyYSkill = SkillCode.SkillY2b;
            return;
        }
        if (Y3.isOn && Y3a.isOn)
        {
            //Soldier.GetComponent<SkillY3>().MyImageScript = IconY.GetComponent<CooldownImage>();
            //Soldier.GetComponent<SkillY3>().enabled = true;
            return;
        }
        if (Y3.isOn && Y3b.isOn)
        {
            //Soldier.GetComponent<SkillY3b>().MyImageScript = IconY.GetComponent<CooldownImage>();
            //Soldier.GetComponent<SkillY3b>().enabled = true;
            return;
        }
    }

    public void AllYOff()
    {
        IconY.GetComponent<CooldownImage>().enabled = false;
        GetComponent<SkillsLink>().KeyYSkill = null;
    }
}
