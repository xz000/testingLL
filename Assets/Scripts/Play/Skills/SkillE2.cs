using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillE2 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    //public float SpeedR2;
    public float maxTimeE2;
    public float pushPower;
    public float pushTime;
    public float pushDamage;
    private float currentcooldown;
    public float cooldowntime = 5;
    public bool skillavaliable;
    MoveScript MS;

    // Use this for initialization
    void Start ()
    {
        currentcooldown = cooldowntime;
        MS = GetComponent<MoveScript>();
    }
	
	// Update is called once per frame
	void GoSkillE2()
    {
        if (skillavaliable)
        {
            GetComponent<DoSkill>().singing = 0;
            Skill();
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
        }
    }

    public void Skill()
    {
        GetComponent<DoSkill>().BeforeSkill();
        MS.controllable = true;
        currentcooldown = 0;
        skillavaliable = false;
        gameObject.GetComponent<ColliderScript>().SetPower(pushPower, pushTime, (Fix64)pushDamage);
        gameObject.GetComponent<ColliderScript>().StartKick(maxTimeE2);
        gameObject.GetComponent<StealthScript>().StealthByTime(maxTimeE2, false);
        GetComponent<StealthScript>().Speed = 1;
    }

    void SkillE2SetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }
}
